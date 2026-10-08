//! The project tree — the single sidebar model, shared by both frontends.
//!
//! Three levels, top to bottom:
//!
//! ```text
//! ▾ Piki                        Project row  (depth 0)
//!   ▾ piki-multi-ai             Repo row     (depth 1, synthetic)
//!       nightly                 Checkout     (depth 2, Primary)
//!       feat/sidebar            Checkout     (depth 2, Worktree)
//!   ▾ piki-multiplex            Repo row
//!       main                    Checkout
//!     ~/notes                   Dir row      (depth 1)
//! ```
//!
//! **Repo rows are synthetic**: they are derived from the `source_repo` of the
//! project's checkout members (plus any explicit [`MemberKind::Repo`] member),
//! never persisted. That is the same grouping rule the old flat workspace
//! sidebar applied globally via `workspace::sidebar_rows` — this function just
//! scopes it to one project's members, which is why switching to a
//! project-only sidebar needs no change to how worktrees are stored.
//!
//! Two synthetic buckets keep the model total, so killing the flat workspace
//! view loses nothing: [`Bucket::PrReview`] collects the `ephemeral` PR-review
//! checkouts (each is its own ad-hoc clone, so repo grouping would emit one
//! useless header per PR) and [`Bucket::Unassigned`] collects every registered
//! workspace no project claims. Core only names them by variant — the display
//! label is the frontend's, so neither string lives here.
//!
//! A workspace that is a member of two projects is emitted under both. Only
//! the `Unassigned` bucket cares about claims.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::domain::{WorkspaceInfo, WorkspaceType};
use crate::projects::{MemberKind, Project};

/// Collapse key for the synthetic PR-review bucket.
pub const PR_REVIEW_KEY: &str = "bucket:pr-review";
/// Collapse key for the synthetic no-project bucket.
pub const UNASSIGNED_KEY: &str = "bucket:unassigned";

/// Which top-level group a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    /// A real project: index into the slice passed to [`project_tree`].
    Project(usize),
    /// Synthetic group for `ephemeral` PR-review checkouts.
    PrReview,
    /// Synthetic group for registered workspaces no project claims.
    Unassigned,
}

/// What a checkout is within its repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutKind {
    /// The original clone (`workspace_type != Worktree`) — at most one per
    /// repo, emitted first.
    Primary,
    /// A `git worktree` sibling.
    Worktree,
}

/// One visual row, in render order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTreeRow {
    /// A top-level header. `checkouts` counts the workspaces underneath it
    /// (across all its repos), so a collapsed header can still show a count.
    Project {
        bucket: Bucket,
        key: String,
        collapsed: bool,
        checkouts: usize,
    },
    /// A repository group. Synthetic — see the module docs. `checkouts` counts
    /// its children; `0` means the repo is a member but nothing of it is
    /// loaded, which is the row a "open / create worktree" action targets.
    Repo {
        bucket: Bucket,
        root: PathBuf,
        display: String,
        key: String,
        collapsed: bool,
        checkouts: usize,
    },
    /// A loaded workspace: `workspace_index` indexes the slice passed to
    /// [`project_tree`]. `depth` is 2 under a repo group and 1 when the row
    /// hangs straight off its header (a non-git workspace, or a PR review).
    Checkout {
        bucket: Bucket,
        workspace_index: usize,
        kind: CheckoutKind,
        depth: u8,
    },
    /// A member path that is neither a loaded workspace nor a repo root.
    Dir { bucket: Bucket, path: PathBuf },
}

impl ProjectTreeRow {
    /// Indentation level, for renderers that don't want to match.
    pub fn depth(&self) -> u8 {
        match self {
            ProjectTreeRow::Project { .. } => 0,
            ProjectTreeRow::Repo { .. } | ProjectTreeRow::Dir { .. } => 1,
            ProjectTreeRow::Checkout { depth, .. } => *depth,
        }
    }

    /// The workspace this row stands for, if any. The one funnel a frontend
    /// uses to turn "the cursor is on this row" into "switch to this
    /// workspace" — nothing else in the tree addresses runtime state.
    pub fn workspace_index(&self) -> Option<usize> {
        match self {
            ProjectTreeRow::Checkout {
                workspace_index, ..
            } => Some(*workspace_index),
            _ => None,
        }
    }

    /// The collapse key of a collapsible row (`Project` and `Repo`).
    pub fn collapse_key(&self) -> Option<&str> {
        match self {
            ProjectTreeRow::Project { key, .. } | ProjectTreeRow::Repo { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Which top-level group this row belongs to.
    pub fn bucket(&self) -> Bucket {
        match self {
            ProjectTreeRow::Project { bucket, .. }
            | ProjectTreeRow::Repo { bucket, .. }
            | ProjectTreeRow::Checkout { bucket, .. }
            | ProjectTreeRow::Dir { bucket, .. } => *bucket,
        }
    }
}

/// Collapse key for a repo group. Scoped by the bucket's own key so collapsing
/// a repo inside one project leaves the same repo expanded in another.
pub fn repo_collapse_key(bucket_key: &str, root: &Path) -> String {
    format!("{bucket_key}|repo:{}", root.to_string_lossy())
}

/// Collapse key for a bucket's header row.
fn bucket_key(bucket: Bucket, projects: &[Project]) -> String {
    match bucket {
        Bucket::Project(i) => projects[i].collapse_key(),
        Bucket::PrReview => PR_REVIEW_KEY.to_string(),
        Bucket::Unassigned => UNASSIGNED_KEY.to_string(),
    }
}

/// What one member resolved to, before grouping.
enum Entry {
    /// A loaded workspace, with its repo root when it should nest under one.
    Checkout { index: usize, repo: Option<PathBuf> },
    /// An explicit repository-root member.
    Repo(PathBuf),
    /// Anything else.
    Dir(PathBuf),
}

/// A pending emission slot, in first-seen order.
enum Slot {
    Repo {
        root: PathBuf,
        /// Workspace indices, in member order.
        members: Vec<usize>,
    },
    /// A checkout that hangs straight off the header (non-git workspace).
    Flat(usize),
    Dir(PathBuf),
}

/// Human-readable name for a repo root. Prefers a member's precomputed
/// `source_repo_display` (which is what the old sidebar showed) and falls back
/// to the last path component.
fn repo_display(root: &Path, workspaces: &[WorkspaceInfo], members: &[usize]) -> String {
    for &i in members {
        let d = &workspaces[i].source_repo_display;
        if !d.is_empty() {
            return d.clone();
        }
    }
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string())
}

/// Should this workspace nest under a repo group?
///
/// A `Simple` workspace pointed at a plain directory has no repository to head
/// a group with (`is_git_repo == false`), so it renders flat under its project
/// instead of inventing a one-row repo header for itself.
fn repo_of(info: &WorkspaceInfo) -> Option<PathBuf> {
    info.is_git_repo.then(|| info.source_repo.clone())
}

/// Emit one bucket: its header plus, unless collapsed, its grouped children.
fn emit_bucket(
    rows: &mut Vec<ProjectTreeRow>,
    bucket: Bucket,
    key: String,
    entries: Vec<Entry>,
    workspaces: &[WorkspaceInfo],
    collapsed: &HashSet<String>,
) {
    // Group by repo root, each group emitted where its first member sat.
    let mut slots: Vec<Slot> = Vec::new();
    for entry in entries {
        match entry {
            Entry::Checkout {
                index,
                repo: Some(root),
            } => {
                match slots
                    .iter_mut()
                    .find(|s| matches!(s, Slot::Repo { root: r, .. } if *r == root))
                {
                    Some(Slot::Repo { members, .. }) => members.push(index),
                    _ => slots.push(Slot::Repo {
                        root,
                        members: vec![index],
                    }),
                }
            }
            Entry::Checkout { index, repo: None } => slots.push(Slot::Flat(index)),
            Entry::Repo(root) => {
                if !slots
                    .iter()
                    .any(|s| matches!(s, Slot::Repo { root: r, .. } if *r == root))
                {
                    slots.push(Slot::Repo {
                        root,
                        members: Vec::new(),
                    });
                }
            }
            Entry::Dir(path) => slots.push(Slot::Dir(path)),
        }
    }

    let checkouts = slots
        .iter()
        .map(|s| match s {
            Slot::Repo { members, .. } => members.len(),
            Slot::Flat(_) => 1,
            Slot::Dir(_) => 0,
        })
        .sum();

    let bucket_collapsed = collapsed.contains(&key);
    rows.push(ProjectTreeRow::Project {
        bucket,
        key: key.clone(),
        collapsed: bucket_collapsed,
        checkouts,
    });
    if bucket_collapsed {
        return;
    }

    for slot in slots {
        match slot {
            Slot::Repo { root, members } => {
                let repo_key = repo_collapse_key(&key, &root);
                let repo_collapsed = collapsed.contains(&repo_key);
                rows.push(ProjectTreeRow::Repo {
                    bucket,
                    display: repo_display(&root, workspaces, &members),
                    root,
                    key: repo_key,
                    collapsed: repo_collapsed,
                    checkouts: members.len(),
                });
                if repo_collapsed {
                    continue;
                }
                // The original clone leads its worktrees, as the flat sidebar
                // did; the rest keep member order.
                let primary = members
                    .iter()
                    .copied()
                    .find(|&i| workspaces[i].workspace_type != WorkspaceType::Worktree);
                if let Some(p) = primary {
                    rows.push(ProjectTreeRow::Checkout {
                        bucket,
                        workspace_index: p,
                        kind: CheckoutKind::Primary,
                        depth: 2,
                    });
                }
                for i in members {
                    if Some(i) == primary {
                        continue;
                    }
                    rows.push(ProjectTreeRow::Checkout {
                        bucket,
                        workspace_index: i,
                        kind: CheckoutKind::Worktree,
                        depth: 2,
                    });
                }
            }
            Slot::Flat(index) => rows.push(ProjectTreeRow::Checkout {
                bucket,
                workspace_index: index,
                kind: CheckoutKind::Primary,
                depth: 1,
            }),
            Slot::Dir(path) => rows.push(ProjectTreeRow::Dir { bucket, path }),
        }
    }
}

/// Build the sidebar's visual rows.
///
/// Bucket order is PR-review first (it is the transient, act-on-it-now group),
/// then the projects in their stored order, then the no-project bucket. A
/// bucket with nothing in it is omitted entirely, header included — except a
/// real project, which always shows so it can be filled.
///
/// `collapsed` holds collapsed keys: [`Project::collapse_key`],
/// [`repo_collapse_key`], [`PR_REVIEW_KEY`] and [`UNASSIGNED_KEY`]. Hidden
/// rows are omitted, so the result is exactly what should be drawn.
pub fn project_tree(
    projects: &[Project],
    workspaces: &[WorkspaceInfo],
    collapsed: &HashSet<String>,
) -> Vec<ProjectTreeRow> {
    let mut rows = Vec::new();

    // PR-review checkouts never group by repo: each is its own ad-hoc clone.
    let review: Vec<Entry> = workspaces
        .iter()
        .enumerate()
        .filter(|(_, w)| w.ephemeral)
        .map(|(i, _)| Entry::Checkout {
            index: i,
            repo: None,
        })
        .collect();
    if !review.is_empty() {
        emit_bucket(
            &mut rows,
            Bucket::PrReview,
            PR_REVIEW_KEY.to_string(),
            review,
            workspaces,
            collapsed,
        );
    }

    // A member resolves to a workspace by exact path.
    let workspace_at = |path: &Path| -> Option<usize> {
        workspaces
            .iter()
            .position(|w| !w.ephemeral && w.path == path)
    };

    let mut claimed: HashSet<usize> = HashSet::new();
    for (pi, project) in projects.iter().enumerate() {
        let mut entries = Vec::with_capacity(project.members.len());
        for member in &project.members {
            match (member.kind, workspace_at(&member.path)) {
                // An explicit repo member whose own clone is also loaded is
                // both: the checkout carries the repo root, so one entry says
                // it all.
                (_, Some(index)) => {
                    claimed.insert(index);
                    entries.push(Entry::Checkout {
                        index,
                        repo: repo_of(&workspaces[index]),
                    });
                }
                (MemberKind::Repo, None) => entries.push(Entry::Repo(member.path.clone())),
                (MemberKind::Auto, None) => entries.push(Entry::Dir(member.path.clone())),
            }
        }
        // Worktrees of a member repo are part of the project even when only
        // the clone was added by hand — that is what makes "create a branch in
        // this project" need no bookkeeping. Appended after the explicit
        // members so hand-set order still leads.
        let repos: Vec<PathBuf> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Checkout { repo: Some(r), .. } => Some(r.clone()),
                Entry::Repo(r) => Some(r.clone()),
                _ => None,
            })
            .collect();
        for (i, w) in workspaces.iter().enumerate() {
            if w.ephemeral || claimed.contains(&i) {
                continue;
            }
            if repos.contains(&w.source_repo) && repo_of(w).is_some() {
                claimed.insert(i);
                entries.push(Entry::Checkout {
                    index: i,
                    repo: Some(w.source_repo.clone()),
                });
            }
        }
        emit_bucket(
            &mut rows,
            Bucket::Project(pi),
            bucket_key(Bucket::Project(pi), projects),
            entries,
            workspaces,
            collapsed,
        );
    }

    let orphans: Vec<Entry> = workspaces
        .iter()
        .enumerate()
        .filter(|(i, w)| !w.ephemeral && !claimed.contains(i))
        .map(|(i, w)| Entry::Checkout {
            index: i,
            repo: repo_of(w),
        })
        .collect();
    if !orphans.is_empty() {
        emit_bucket(
            &mut rows,
            Bucket::Unassigned,
            UNASSIGNED_KEY.to_string(),
            orphans,
            workspaces,
            collapsed,
        );
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectMember;

    fn ws(name: &str, repo: &str, kind: WorkspaceType) -> WorkspaceInfo {
        WorkspaceInfo {
            name: name.to_string(),
            path: PathBuf::from(format!("/wt/{name}")),
            workspace_type: kind,
            description: String::new(),
            prompt: String::new(),
            kanban_path: None,
            order: 0,
            source_repo: PathBuf::from(repo),
            source_repo_display: repo.rsplit('/').next().unwrap_or(repo).to_string(),
            dispatch_card_id: None,
            dispatch_source_kanban: None,
            dispatch_agent_name: None,
            origin: Default::default(),
            is_git_repo: true,
            ephemeral: false,
            pr_repo_nwo: None,
            pr_number: None,
        }
    }

    fn clone_of(name: &str, repo: &str) -> WorkspaceInfo {
        ws(name, repo, WorkspaceType::Simple)
    }

    fn worktree(name: &str, repo: &str) -> WorkspaceInfo {
        ws(name, repo, WorkspaceType::Worktree)
    }

    fn plain_dir_ws(name: &str) -> WorkspaceInfo {
        let mut w = ws(name, &format!("/dirs/{name}"), WorkspaceType::Simple);
        w.is_git_repo = false;
        w
    }

    fn review(name: &str) -> WorkspaceInfo {
        let mut w = ws(name, &format!("/pr/{name}"), WorkspaceType::Simple);
        w.ephemeral = true;
        w
    }

    fn project(id: i64, name: &str, members: Vec<ProjectMember>) -> Project {
        Project {
            id: Some(id),
            name: name.to_string(),
            color: 0,
            order: 0,
            members,
        }
    }

    fn none() -> HashSet<String> {
        HashSet::new()
    }

    /// Compact render of the tree: `depth` dots + a label per row.
    fn sketch(rows: &[ProjectTreeRow], workspaces: &[WorkspaceInfo]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let indent = ".".repeat(r.depth() as usize);
                let label = match r {
                    ProjectTreeRow::Project { bucket, .. } => format!("{bucket:?}"),
                    ProjectTreeRow::Repo { display, .. } => format!("repo {display}"),
                    ProjectTreeRow::Checkout {
                        workspace_index,
                        kind,
                        ..
                    } => format!("{} {kind:?}", workspaces[*workspace_index].name),
                    ProjectTreeRow::Dir { path, .. } => format!("dir {}", path.display()),
                };
                format!("{indent}{label}")
            })
            .collect()
    }

    #[test]
    fn a_project_groups_its_checkouts_under_synthetic_repo_rows() {
        let list = [
            worktree("feature", "/repos/app"),
            clone_of("app", "/repos/app"),
            clone_of("lib", "/repos/lib"),
        ];
        // Declared out of order on purpose: the clone still leads its repo.
        let projects = [project(
            1,
            "Piki",
            vec![
                ProjectMember::new(PathBuf::from("/wt/feature")),
                ProjectMember::new(PathBuf::from("/wt/app")),
                ProjectMember::new(PathBuf::from("/wt/lib")),
            ],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                ".repo app",
                "..app Primary",
                "..feature Worktree",
                ".repo lib",
                "..lib Primary",
            ]
        );
    }

    #[test]
    fn one_project_can_hold_several_repos_and_a_plain_dir() {
        let list = [clone_of("api", "/repos/api"), plain_dir_ws("notes")];
        let projects = [project(
            1,
            "ACME",
            vec![
                ProjectMember::new(PathBuf::from("/wt/api")),
                ProjectMember::repo(PathBuf::from("/repos/web")),
                ProjectMember::new(PathBuf::from("/wt/notes")),
                ProjectMember::new(PathBuf::from("/home/me/scratch")),
            ],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                ".repo api",
                "..api Primary",
                // A repo member with nothing loaded still gets its row.
                ".repo web",
                // A non-git workspace has no repo to head a group with.
                ".notes Primary",
                ".dir /home/me/scratch",
            ]
        );
        let unloaded = rows
            .iter()
            .find(|r| matches!(r, ProjectTreeRow::Repo { display, .. } if display == "web"))
            .unwrap();
        assert!(matches!(
            unloaded,
            ProjectTreeRow::Repo { checkouts: 0, .. }
        ));
    }

    /// The whole point of the model: a worktree created in a member repo shows
    /// up in the project without anyone adding it as a member.
    #[test]
    fn worktrees_of_a_member_repo_join_the_project_automatically() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                ".repo app",
                "..app Primary",
                "..feature Worktree"
            ]
        );
        assert!(
            !rows.iter().any(|r| r.bucket() == Bucket::Unassigned),
            "nothing is left over"
        );
    }

    #[test]
    fn one_repo_can_belong_to_two_projects() {
        let list = [clone_of("app", "/repos/app")];
        let projects = [
            project(1, "A", vec![ProjectMember::new(PathBuf::from("/wt/app"))]),
            project(2, "B", vec![ProjectMember::new(PathBuf::from("/wt/app"))]),
        ];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                ".repo app",
                "..app Primary",
                "Project(1)",
                ".repo app",
                "..app Primary",
            ]
        );
    }

    #[test]
    fn repos_collapse_independently_per_project() {
        let list = [clone_of("app", "/repos/app")];
        let projects = [
            project(1, "A", vec![ProjectMember::new(PathBuf::from("/wt/app"))]),
            project(2, "B", vec![ProjectMember::new(PathBuf::from("/wt/app"))]),
        ];
        let collapsed = HashSet::from([repo_collapse_key("project:1", Path::new("/repos/app"))]);
        let rows = project_tree(&projects, &list, &collapsed);
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                ".repo app",
                "Project(1)",
                ".repo app",
                "..app Primary",
            ]
        );
    }

    #[test]
    fn collapsing_a_project_hides_everything_under_it_but_keeps_the_count() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        let collapsed = HashSet::from(["project:1".to_string()]);
        let rows = project_tree(&projects, &list, &collapsed);
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows[0],
            ProjectTreeRow::Project {
                collapsed: true,
                checkouts: 2,
                ..
            }
        ));
    }

    #[test]
    fn workspaces_no_project_claims_land_in_the_unassigned_bucket() {
        let list = [clone_of("app", "/repos/app"), clone_of("other", "/repos/x")];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                ".repo app",
                "..app Primary",
                "Unassigned",
                ".repo x",
                "..other Primary",
            ]
        );
    }

    #[test]
    fn with_no_projects_at_all_everything_is_unassigned() {
        let list = [clone_of("app", "/repos/app")];
        let rows = project_tree(&[], &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec!["Unassigned", ".repo app", "..app Primary"]
        );
    }

    #[test]
    fn an_empty_project_still_shows_its_header() {
        let projects = [project(1, "Empty", vec![])];
        let rows = project_tree(&projects, &[], &none());
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows[0],
            ProjectTreeRow::Project { checkouts: 0, .. }
        ));
    }

    #[test]
    fn pr_reviews_lead_the_tree_flat_under_their_own_bucket() {
        let list = [
            clone_of("app", "/repos/app"),
            review("pr-1"),
            review("pr-2"),
        ];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "PrReview",
                ".pr-1 Primary",
                ".pr-2 Primary",
                "Project(0)",
                ".repo app",
                "..app Primary",
            ]
        );
    }

    #[test]
    fn a_review_checkout_is_never_also_a_project_member_row() {
        let mut review_in_repo = review("pr-9");
        review_in_repo.source_repo = PathBuf::from("/repos/app");
        let list = [clone_of("app", "/repos/app"), review_in_repo];
        let projects = [project(
            1,
            "Piki",
            vec![
                ProjectMember::new(PathBuf::from("/wt/app")),
                // Even named outright, it stays in the review bucket.
                ProjectMember::new(PathBuf::from("/wt/pr-9")),
            ],
        )];
        let rows = project_tree(&projects, &list, &none());
        let appearances = rows
            .iter()
            .filter(|r| r.workspace_index() == Some(1))
            .count();
        assert_eq!(appearances, 1, "{rows:#?}");
        assert_eq!(rows[0].bucket(), Bucket::PrReview);
    }

    #[test]
    fn collapsing_the_review_bucket_hides_its_members() {
        let list = [review("pr-1")];
        let collapsed = HashSet::from([PR_REVIEW_KEY.to_string()]);
        let rows = project_tree(&[], &list, &collapsed);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bucket(), Bucket::PrReview);
    }

    #[test]
    fn empty_input_produces_no_rows() {
        assert!(project_tree(&[], &[], &none()).is_empty());
    }

    #[test]
    fn checkout_rows_are_the_only_ones_addressing_a_workspace() {
        let list = [clone_of("app", "/repos/app")];
        let rows = project_tree(&[], &list, &none());
        let addressed: Vec<usize> = rows.iter().filter_map(|r| r.workspace_index()).collect();
        assert_eq!(addressed, vec![0]);
    }
}
