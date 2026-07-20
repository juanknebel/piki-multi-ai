//! Listing and categorizing PRs relevant to the current `gh` user, so Code
//! Review can be entered from anywhere without an existing checkout.

use serde::Deserialize;

/// Why a PR showed up in [`list_relevant_prs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrInclusionReason {
    /// The user opened the PR.
    Authored,
    /// The user already commented and/or reviewed it. `review_requested` is
    /// `true` when GitHub's timeline shows the user was asked to review (as
    /// opposed to commenting unprompted).
    Interacted { review_requested: bool },
    /// A review was requested and the user hasn't interacted yet.
    ReviewRequestedPending,
}

/// One row in the PR picker.
#[derive(Debug, Clone)]
pub struct PrListItem {
    pub number: u64,
    pub title: String,
    /// "owner/repo"
    pub repo_nwo: String,
    pub url: String,
    pub author: String,
    pub is_draft: bool,
    pub updated_at: String,
    pub reason: PrInclusionReason,
}

/// Raw shape of one row from `gh search prs --json ...`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchPr {
    pub number: u64,
    pub title: String,
    pub repository: SearchPrRepo,
    pub url: String,
    pub author: SearchPrAuthor,
    #[serde(default)]
    pub is_draft: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchPrRepo {
    pub name_with_owner: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct SearchPrAuthor {
    #[serde(default)]
    pub login: String,
}

const SEARCH_JSON_FIELDS: &str = "number,title,repository,url,author,isDraft,updatedAt";

async fn search_prs(extra_args: &[&str]) -> anyhow::Result<Vec<SearchPr>> {
    let mut args = vec!["search", "prs", "--state", "open", "--json", SEARCH_JSON_FIELDS];
    args.extend_from_slice(extra_args);

    let output = crate::shell_env::command("gh").args(&args).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(stderr = %stderr.trim(), args = ?args, "gh search prs failed");
        anyhow::bail!("gh search prs failed: {}", stderr.trim());
    }

    let prs: Vec<SearchPr> = serde_json::from_slice(&output.stdout)?;
    Ok(prs)
}

/// Fetch and categorize PRs relevant to the current `gh` user across all
/// accessible repos: authored, interacted-with (commented/reviewed), and
/// review-requested-but-pending. Runs four `gh search prs` calls in parallel.
pub async fn list_relevant_prs(limit: usize) -> anyhow::Result<Vec<PrListItem>> {
    let limit_str = limit.to_string();
    let author_args = ["--author", "@me", "--limit", &limit_str];
    let requested_args = ["--review-requested", "@me", "--limit", &limit_str];
    let reviewed_args = ["--reviewed-by", "@me", "--limit", &limit_str];
    let commented_args = ["--commenter", "@me", "--limit", &limit_str];

    let (authored, requested, reviewed, commented) = tokio::try_join!(
        search_prs(&author_args),
        search_prs(&requested_args),
        search_prs(&reviewed_args),
        search_prs(&commented_args),
    )?;

    let mut items = categorize_prs(authored, requested, reviewed, commented);

    // Best-effort exact "was a review requested" check, only for PRs already
    // interacted with (bounds the number of extra timeline calls).
    for item in items.iter_mut() {
        if let PrInclusionReason::Interacted { review_requested } = &mut item.reason {
            *review_requested = was_review_requested(&item.repo_nwo, item.number)
                .await
                .unwrap_or(false);
        }
    }

    Ok(items)
}

fn pr_key(pr: &SearchPr) -> (String, u64) {
    (pr.repository.name_with_owner.clone(), pr.number)
}

fn into_item(pr: SearchPr, reason: PrInclusionReason) -> PrListItem {
    PrListItem {
        number: pr.number,
        title: pr.title,
        repo_nwo: pr.repository.name_with_owner,
        url: pr.url,
        author: pr.author.login,
        is_draft: pr.is_draft,
        updated_at: pr.updated_at,
        reason,
    }
}

/// Pure categorization: authored > interacted (reviewed ∪ commented) >
/// review-requested-pending, deduped by (repo, number), each bucket sorted by
/// `updated_at` descending.
pub(super) fn categorize_prs(
    authored: Vec<SearchPr>,
    requested: Vec<SearchPr>,
    reviewed: Vec<SearchPr>,
    commented: Vec<SearchPr>,
) -> Vec<PrListItem> {
    use std::collections::{HashMap, HashSet};

    let authored_keys: HashSet<(String, u64)> = authored.iter().map(pr_key).collect();
    let requested_keys: HashSet<(String, u64)> = requested.iter().map(pr_key).collect();

    let mut interacted: HashMap<(String, u64), SearchPr> = HashMap::new();
    for pr in reviewed.into_iter().chain(commented) {
        let key = pr_key(&pr);
        if authored_keys.contains(&key) {
            continue;
        }
        interacted.entry(key).or_insert(pr);
    }

    let mut out = Vec::new();

    let mut authored_sorted = authored;
    authored_sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out.extend(authored_sorted.into_iter().map(|pr| into_item(pr, PrInclusionReason::Authored)));

    let mut interacted_sorted: Vec<SearchPr> = interacted.into_values().collect();
    interacted_sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out.extend(interacted_sorted.into_iter().map(|pr| {
        let review_requested = requested_keys.contains(&pr_key(&pr));
        into_item(pr, PrInclusionReason::Interacted { review_requested })
    }));

    let interacted_or_authored: HashSet<(String, u64)> =
        out.iter().map(|i| (i.repo_nwo.clone(), i.number)).collect();
    let mut pending_sorted: Vec<SearchPr> = requested
        .into_iter()
        .filter(|pr| !interacted_or_authored.contains(&pr_key(pr)))
        .collect();
    pending_sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out.extend(
        pending_sorted
            .into_iter()
            .map(|pr| into_item(pr, PrInclusionReason::ReviewRequestedPending)),
    );

    out
}

#[derive(Debug, Deserialize)]
struct TimelineEvent {
    event: String,
    #[serde(default)]
    requested_reviewer: Option<TimelineUser>,
    #[serde(default)]
    requested_team: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct TimelineUser {
    #[serde(default)]
    login: String,
}

/// Exact check via the issue timeline API: was a `review_requested` event for
/// `@me` ever recorded on this PR? Costs one extra `gh api` call per PR, so
/// callers should only use it for the "already interacted" bucket.
async fn was_review_requested(repo_nwo: &str, number: u64) -> anyhow::Result<bool> {
    let me = current_login().await?;
    let endpoint = format!("repos/{repo_nwo}/issues/{number}/timeline");
    let output = crate::shell_env::command("gh")
        .args(["api", "--paginate", &endpoint])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh api {endpoint} failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut de = serde_json::Deserializer::from_str(&stdout).into_iter::<Vec<TimelineEvent>>();
    for page in &mut de {
        let page = page?;
        for ev in page {
            if ev.event != "review_requested" {
                continue;
            }
            let requested_me = ev
                .requested_reviewer
                .as_ref()
                .map(|u| u.login == me)
                .unwrap_or(false);
            // Team-based requests can't be resolved to "was it me" without an
            // extra membership lookup; treat them as not-me (best-effort).
            let _ = &ev.requested_team;
            if requested_me {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

async fn current_login() -> anyhow::Result<String> {
    let output = crate::shell_env::command("gh")
        .args(["api", "user", "-q", ".login"])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh api user failed: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(nwo: &str, number: u64, updated_at: &str) -> SearchPr {
        SearchPr {
            number,
            title: format!("PR #{number}"),
            repository: SearchPrRepo { name_with_owner: nwo.to_string() },
            url: format!("https://github.com/{nwo}/pull/{number}"),
            author: SearchPrAuthor { login: "someone".to_string() },
            is_draft: false,
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn authored_excluded_from_other_buckets() {
        let authored = vec![pr("o/r", 1, "2026-01-01T00:00:00Z")];
        let reviewed = vec![pr("o/r", 1, "2026-01-01T00:00:00Z")];
        let items = categorize_prs(authored, vec![], reviewed, vec![]);

        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].reason, PrInclusionReason::Authored));
    }

    #[test]
    fn interacted_dedupes_reviewed_and_commented() {
        let reviewed = vec![pr("o/r", 2, "2026-01-01T00:00:00Z")];
        let commented = vec![pr("o/r", 2, "2026-01-01T00:00:00Z")];
        let items = categorize_prs(vec![], vec![], reviewed, commented);

        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0].reason,
            PrInclusionReason::Interacted { review_requested: false }
        ));
    }

    #[test]
    fn interacted_flags_review_requested_when_pending() {
        let requested = vec![pr("o/r", 3, "2026-01-01T00:00:00Z")];
        let commented = vec![pr("o/r", 3, "2026-01-01T00:00:00Z")];
        let items = categorize_prs(vec![], requested, vec![], commented);

        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0].reason,
            PrInclusionReason::Interacted { review_requested: true }
        ));
    }

    #[test]
    fn review_requested_pending_excludes_interacted_and_authored() {
        let authored = vec![pr("o/r", 1, "2026-01-01T00:00:00Z")];
        let requested = vec![
            pr("o/r", 1, "2026-01-01T00:00:00Z"), // authored -> excluded here
            pr("o/r", 2, "2026-01-02T00:00:00Z"), // interacted -> excluded here
            pr("o/r", 4, "2026-01-03T00:00:00Z"), // pending
        ];
        let reviewed = vec![pr("o/r", 2, "2026-01-02T00:00:00Z")];
        let items = categorize_prs(authored, requested, reviewed, vec![]);

        let pending: Vec<_> = items
            .iter()
            .filter(|i| matches!(i.reason, PrInclusionReason::ReviewRequestedPending))
            .collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].number, 4);
    }

    #[test]
    fn buckets_sorted_by_updated_at_desc() {
        let authored = vec![
            pr("o/r", 1, "2026-01-01T00:00:00Z"),
            pr("o/r", 2, "2026-03-01T00:00:00Z"),
            pr("o/r", 3, "2026-02-01T00:00:00Z"),
        ];
        let items = categorize_prs(authored, vec![], vec![], vec![]);
        let numbers: Vec<u64> = items.iter().map(|i| i.number).collect();
        assert_eq!(numbers, vec![2, 3, 1]);
    }

    #[test]
    fn dedupes_across_repos_by_repo_and_number() {
        // Same PR number in two different repos must not collide.
        let reviewed = vec![pr("o/r1", 5, "2026-01-01T00:00:00Z")];
        let commented = vec![pr("o/r2", 5, "2026-01-01T00:00:00Z")];
        let items = categorize_prs(vec![], vec![], reviewed, commented);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn search_pr_json_deserializes() {
        let raw = r#"[{
            "number": 42,
            "title": "Fix bug",
            "repository": {"name": "repo", "nameWithOwner": "owner/repo"},
            "url": "https://github.com/owner/repo/pull/42",
            "author": {"id": "abc", "is_bot": false, "login": "octocat", "name": "The Octocat", "type": "User"},
            "isDraft": false,
            "updatedAt": "2026-07-01T12:00:00Z"
        }]"#;
        let prs: Vec<SearchPr> = serde_json::from_str(raw).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 42);
        assert_eq!(prs[0].repository.name_with_owner, "owner/repo");
        assert_eq!(prs[0].author.login, "octocat");
        assert!(!prs[0].is_draft);
    }

    #[test]
    fn timeline_event_deserializes_review_requested() {
        let raw = r#"[{
            "event": "review_requested",
            "requested_reviewer": {"login": "octocat"}
        }, {
            "event": "commented"
        }]"#;
        let events: Vec<TimelineEvent> = serde_json::from_str(raw).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "review_requested");
        assert_eq!(events[0].requested_reviewer.as_ref().unwrap().login, "octocat");
    }
}
