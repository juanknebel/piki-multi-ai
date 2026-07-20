//! Efficient, idempotent checkout of a PR into an app-managed directory, so
//! Code Review never depends on the user having already cloned/checked out
//! the repo themselves.
//!
//! Layout under the manager's `root` (normally `DataPaths::review_checkouts_dir()`):
//! ```text
//! <root>/<owner>__<repo>/            full clone, shared across PRs of that repo
//! <root>/<owner>__<repo>--pr-<N>/    one `git worktree` per PR being reviewed
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::PrInfo;

pub struct ReviewCheckoutManager {
    root: PathBuf,
}

pub struct PrCheckout {
    /// Working directory for this PR — pass this wherever the rest of
    /// `github` expects a checkout path (`get_pr_file_diff_raw`, `get_pr_review_comments`, ...).
    pub path: PathBuf,
    pub repo_nwo: String,
    pub pr: PrInfo,
    /// What to diff against, e.g. `"origin/main"` — pass as `base_ref` to
    /// [`super::get_pr_file_diff_raw`].
    pub base_spec: String,
    /// `true` if nothing needed to be fetched/reset — the local checkout was
    /// already at the PR's current head with a clean working tree.
    pub reused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutPlan {
    /// Local checkout is already at `remote_head` and clean — do nothing.
    Reuse,
    /// A worktree exists but is stale or dirty — fetch + reset + clean it.
    UpdateExisting,
    /// No usable worktree exists yet (or it's corrupt) — (re)create it.
    CloneAndCheckout,
}

/// Pure decision function, kept separate from any I/O so it's cheaply testable.
pub fn plan_checkout(
    exists: bool,
    local_head: Option<&str>,
    remote_head: &str,
    dirty: bool,
) -> CheckoutPlan {
    if !exists {
        return CheckoutPlan::CloneAndCheckout;
    }
    let Some(local_head) = local_head else {
        // `.git` present but HEAD unreadable — treat as corrupt.
        return CheckoutPlan::CloneAndCheckout;
    };
    if dirty || local_head != remote_head {
        return CheckoutPlan::UpdateExisting;
    }
    CheckoutPlan::Reuse
}

/// `"owner/repo"` -> `"owner__repo"`, safe as a single path component.
pub fn sanitize_nwo(nwo: &str) -> String {
    nwo.replace('/', "__")
}

fn repo_clone_url(nwo: &str) -> String {
    format!("https://github.com/{nwo}.git")
}

fn pr_head_ref(number: u64) -> String {
    format!("refs/piki-review/pr-{number}")
}

async fn run_git(dir: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = crate::shell_env::command("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await?;
    Ok(output)
}

async fn run_git_ok(dir: &Path, args: &[&str], what: &str) -> anyhow::Result<()> {
    let output = run_git(dir, args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{what} failed: {}", stderr.trim());
    }
    Ok(())
}

async fn git_rev_parse_head(dir: &Path) -> Option<String> {
    let output = run_git(dir, &["rev-parse", "HEAD"]).await.ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn git_is_dirty(dir: &Path) -> anyhow::Result<bool> {
    let output = run_git(dir, &["status", "--porcelain"]).await?;
    if !output.status.success() {
        // Unreadable status = treat as dirty so the caller re-creates it.
        return Ok(true);
    }
    Ok(!output.stdout.is_empty())
}

impl ReviewCheckoutManager {
    /// `root` is normally `DataPaths::review_checkouts_dir()`; injectable so
    /// tests can point at a temp dir.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn base_clone_dir(&self, repo_nwo: &str) -> PathBuf {
        self.root.join(sanitize_nwo(repo_nwo))
    }

    fn pr_worktree_dir(&self, repo_nwo: &str, number: u64) -> PathBuf {
        self.root.join(format!("{}--pr-{number}", sanitize_nwo(repo_nwo)))
    }

    async fn ensure_base_clone(&self, repo_nwo: &str, base_dir: &Path) -> anyhow::Result<()> {
        let git_dir_ok = base_dir.join(".git").is_dir()
            && run_git(base_dir, &["rev-parse", "--git-dir"]).await.map(|o| o.status.success()).unwrap_or(false);

        if git_dir_ok {
            return Ok(());
        }
        if base_dir.exists() {
            tokio::fs::remove_dir_all(base_dir).await.ok();
        }
        if let Some(parent) = base_dir.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let url = repo_clone_url(repo_nwo);
        let output = crate::shell_env::command("git")
            .args(["clone", &url, base_dir.to_str().unwrap()])
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!("git clone {url} failed: {}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(())
    }

    async fn fetch_pr_refs(&self, base_dir: &Path, number: u64, base_ref_name: &str) -> anyhow::Result<String> {
        let head_ref = pr_head_ref(number);
        let refspec_head = format!("refs/pull/{number}/head:{head_ref}");
        let refspec_base = format!("+{base_ref_name}:refs/remotes/origin/{base_ref_name}");
        run_git_ok(
            base_dir,
            &["fetch", "origin", &refspec_head, &refspec_base],
            "git fetch",
        )
        .await?;
        Ok(head_ref)
    }

    async fn recreate_worktree(&self, base_dir: &Path, worktree_dir: &Path, head_ref: &str) -> anyhow::Result<()> {
        // Best-effort teardown of any stale registration/directory.
        let worktree_str = worktree_dir.to_str().unwrap();
        let _ = run_git(base_dir, &["worktree", "remove", "--force", worktree_str]).await;
        if worktree_dir.exists() {
            tokio::fs::remove_dir_all(worktree_dir).await.ok();
        }
        let _ = run_git(base_dir, &["worktree", "prune"]).await;

        run_git_ok(
            base_dir,
            &["worktree", "add", "--detach", worktree_str, head_ref],
            "git worktree add",
        )
        .await
    }

    /// Idempotent: fetches the PR's current head SHA via `gh` (no git I/O),
    /// then reuses the existing checkout if it's already at that SHA and
    /// clean, or fetches+resets it in place, or clones+checks it out fresh.
    pub async fn ensure_pr_checkout(&self, repo_nwo: &str, number: u64) -> anyhow::Result<PrCheckout> {
        let pr = super::get_pr_view(repo_nwo, number).await?;
        if pr.state != "OPEN" {
            anyhow::bail!("PR #{number} is not open (state: {})", pr.state);
        }

        let base_dir = self.base_clone_dir(repo_nwo);
        let worktree_dir = self.pr_worktree_dir(repo_nwo, number);

        let exists = worktree_dir.join(".git").exists();
        let (local_head, dirty) = if exists {
            let head = git_rev_parse_head(&worktree_dir).await;
            let dirty = git_is_dirty(&worktree_dir).await.unwrap_or(true);
            (head, dirty)
        } else {
            (None, false)
        };

        let plan = plan_checkout(exists, local_head.as_deref(), &pr.head_ref_oid, dirty);
        let reused = plan == CheckoutPlan::Reuse;

        match plan {
            CheckoutPlan::Reuse => {}
            CheckoutPlan::UpdateExisting => {
                self.ensure_base_clone(repo_nwo, &base_dir).await?;
                let head_ref = self.fetch_pr_refs(&base_dir, number, &pr.base_ref_name).await?;
                run_git_ok(&worktree_dir, &["reset", "--hard", &head_ref], "git reset --hard").await?;
                run_git_ok(&worktree_dir, &["clean", "-fd"], "git clean").await?;
            }
            CheckoutPlan::CloneAndCheckout => {
                self.ensure_base_clone(repo_nwo, &base_dir).await?;
                let head_ref = self.fetch_pr_refs(&base_dir, number, &pr.base_ref_name).await?;
                self.recreate_worktree(&base_dir, &worktree_dir, &head_ref).await?;
            }
        }

        Ok(PrCheckout {
            path: worktree_dir,
            repo_nwo: repo_nwo.to_string(),
            base_spec: format!("origin/{}", pr.base_ref_name),
            pr,
            reused,
        })
    }

    /// Removes a PR's worktree (used when its review workspace is closed).
    /// The shared base clone is left in place for other PRs of the same repo.
    pub async fn remove_checkout(&self, repo_nwo: &str, number: u64) -> anyhow::Result<()> {
        let base_dir = self.base_clone_dir(repo_nwo);
        let worktree_dir = self.pr_worktree_dir(repo_nwo, number);
        if base_dir.exists() {
            let _ = run_git(&base_dir, &["worktree", "remove", "--force", worktree_dir.to_str().unwrap()]).await;
            let _ = run_git(&base_dir, &["worktree", "prune"]).await;
        }
        if worktree_dir.exists() {
            tokio::fs::remove_dir_all(&worktree_dir).await?;
        }
        Ok(())
    }

    /// Best-effort GC: removes top-level entries under `root` untouched for
    /// longer than `max_age`. Safe to call on startup — mtime is refreshed by
    /// every `ensure_pr_checkout` fetch/reset.
    pub async fn prune_stale(&self, max_age: Duration) -> anyhow::Result<usize> {
        if !self.root.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified = match metadata.modified() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let age = std::time::SystemTime::now().duration_since(modified).unwrap_or_default();
            if age > max_age && tokio::fs::remove_dir_all(entry.path()).await.is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_no_existing_checkout_clones() {
        assert_eq!(plan_checkout(false, None, "abc", false), CheckoutPlan::CloneAndCheckout);
    }

    #[test]
    fn plan_corrupt_checkout_reclones() {
        assert_eq!(plan_checkout(true, None, "abc", false), CheckoutPlan::CloneAndCheckout);
    }

    #[test]
    fn plan_clean_and_up_to_date_reuses() {
        assert_eq!(plan_checkout(true, Some("abc"), "abc", false), CheckoutPlan::Reuse);
    }

    #[test]
    fn plan_stale_head_updates() {
        assert_eq!(plan_checkout(true, Some("old"), "new", false), CheckoutPlan::UpdateExisting);
    }

    #[test]
    fn plan_dirty_updates_even_if_head_matches() {
        assert_eq!(plan_checkout(true, Some("abc"), "abc", true), CheckoutPlan::UpdateExisting);
    }

    #[test]
    fn sanitize_nwo_replaces_slash() {
        assert_eq!(sanitize_nwo("owner/repo"), "owner__repo");
    }

    #[test]
    fn dir_naming_avoids_collisions() {
        let mgr = ReviewCheckoutManager::new(PathBuf::from("/tmp/piki-review-test"));
        let base = mgr.base_clone_dir("owner/repo");
        let worktree = mgr.pr_worktree_dir("owner/repo", 42);
        assert_eq!(base, PathBuf::from("/tmp/piki-review-test/owner__repo"));
        assert_eq!(worktree, PathBuf::from("/tmp/piki-review-test/owner__repo--pr-42"));
        assert_ne!(base, worktree);
    }
}
