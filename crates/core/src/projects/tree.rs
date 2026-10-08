//! The project tree — the single sidebar model, shared by both frontends.
//!
//! Three levels, top to bottom:
//!
//! ```text
//! ▾ Piki                        Project row  (depth 0)
//!   ▾ piki-multi-ai             Repo row     (depth 1, synthetic)
//!       main                    Checkout     (depth 2, Primary)
//!       feat/sidebar            Checkout     (depth 2, Worktree)
//!     piki-multiplex (main)     Checkout     (depth 1, HOISTED)
//!     ~/notes                   Dir row      (depth 1)
//! ⎇ dirb (main)                 Checkout     (depth 0, loose + HOISTED)
//! ```
//!
//! A repository with a **single** loaded checkout is *hoisted*: its header
//! would only repeat the checkout's name and have nothing to collapse, so the
//! checkout stands in for the repo (`Checkout { hoisted: true }`, named after
//! the repository folder with its branch alongside). The group re-splits into
//! header + children by itself as soon as a second checkout exists — the tree
//! is derived on every render, so creating a worktree is all it takes. That
//! keeps the common case (one repo, one clone) exactly as compact as the flat
//! sidebar this replaced, and spends the extra level only where there is
//! actually a branch tree to show.
//!
//! **Repo rows are synthetic**: they are derived from the `source_repo` of the
//! project's checkout members (plus any explicit [`MemberKind::Repo`] member),
//! never persisted. That is the same grouping rule the old flat workspace
//! sidebar applied globally via `workspace::sidebar_rows` — this function just
//! scopes it to one project's members, which is why switching to a
//! project-only sidebar needs no change to how worktrees are stored.
//!
//! Two synthetic buckets keep the model total, so killing the flat workspace
//! view loses nothing. [`Bucket::PrReview`] collects the `ephemeral` PR-review
//! checkouts under a header of its own (each is its own ad-hoc clone, so repo
//! grouping would emit one useless header per PR); core names it by variant,
//! the display label is the frontend's.
//!
//! [`Bucket::Unassigned`] — every registered workspace no project claims —
//! gets **no header at all**: its repositories are emitted at depth 0, after
//! the projects. A repository you have not grouped with anything is not "in"
//! some pseudo-project, and wrapping it in one only added a row whose name
//! repeated the repository's. So the sidebar reads as "my projects, then my
//! loose repositories", which is what a project being a *choice* means.
//!
//! **A project holds exactly the checkouts you put in it.** Membership is
//! never inferred: two worktrees of one repository can live in different
//! projects, or one in a project and the other nowhere, because which *branch*
//! belongs to a piece of work is the user's call and not something a shared
//! `source_repo` can answer. An earlier version pulled in every sibling
//! checkout of a member repository "so creating a branch needs no
//! bookkeeping"; it silently dragged a repo's main clone into a project that
//! had only been given one worktree. The convenience is paid for explicitly
//! instead: creating a branch from inside a project adds it as a member there
//! (see the frontends' `pending_project_member`).
//!
//! A workspace that is a member of two projects is emitted under both. Only
//! the `Unassigned` bucket cares about claims.

use std::collections::{HashMap, HashSet};
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
    /// A top-level header. Deliberately carries no checkout count: the number
    /// said nothing a glance at the rows (or at the collapsed group's rolled-up
    /// attention signals) doesn't, and it competed with them for the one spot
    /// on the right of the row that matters.
    Project {
        bucket: Bucket,
        key: String,
        collapsed: bool,
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
        /// 1 under a project header, 0 for a loose repository (the no-project
        /// bucket has no header to sit under).
        depth: u8,
    },
    /// A loaded workspace: `workspace_index` indexes the slice passed to
    /// [`project_tree`]. `depth` is one past its repo group's, or its group's
    /// own when hoisted — so a loose hoisted repository is 0 and a checkout
    /// under a project's repo group is 2.
    Checkout {
        bucket: Bucket,
        workspace_index: usize,
        kind: CheckoutKind,
        depth: u8,
        /// This row *is* its repository: the group had a single checkout, so
        /// emitting a header above it would have cost two rows saying the same
        /// thing (see [`project_tree`]). Renderers label a hoisted row by the
        /// repository folder with its branch alongside — the way the old flat
        /// sidebar named a clone — and repository actions ("new branch") apply
        /// to it, since there is no header to carry them.
        hoisted: bool,
        /// There is a [`ProjectTreeRow::Repo`] header directly above this row.
        /// Renderers key the branch-only label and the tree guide on THIS, not
        /// on `depth`: the same child sits at depth 2 under a project and at
        /// depth 1 under a loose repository, and both must read the same.
        in_group: bool,
    },
    /// A member path that is neither a loaded workspace nor a repo root.
    Dir { bucket: Bucket, path: PathBuf },
}

impl ProjectTreeRow {
    /// Indentation level, for renderers that don't want to match.
    pub fn depth(&self) -> u8 {
        match self {
            ProjectTreeRow::Project { .. } => 0,
            ProjectTreeRow::Dir { .. } => 1,
            ProjectTreeRow::Repo { depth, .. } | ProjectTreeRow::Checkout { depth, .. } => *depth,
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

    /// Whether a repository header sits directly above this row — see
    /// `Checkout::in_group`.
    pub fn is_in_group(&self) -> bool {
        matches!(self, ProjectTreeRow::Checkout { in_group: true, .. })
    }

    /// Whether this row stands in for its whole repository — see
    /// [`ProjectTreeRow::Checkout::hoisted`].
    pub fn is_hoisted(&self) -> bool {
        matches!(self, ProjectTreeRow::Checkout { hoisted: true, .. })
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

/// What a checkout is by its own type, for the rows where no sibling decides
/// it (a hoisted repo, a flat non-git workspace).
fn checkout_kind(info: &WorkspaceInfo) -> CheckoutKind {
    if info.workspace_type == WorkspaceType::Worktree {
        CheckoutKind::Worktree
    } else {
        CheckoutKind::Primary
    }
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

    // The no-project bucket has no header: a repository you have not grouped
    // with anything is not "in" a pseudo-project, and the header only added a
    // row repeating the repository's own name. Its rows therefore sit one
    // level further out than a project's.
    let headed = bucket != Bucket::Unassigned;
    if headed {
        let bucket_collapsed = collapsed.contains(&key);
        rows.push(ProjectTreeRow::Project {
            bucket,
            key: key.clone(),
            collapsed: bucket_collapsed,
        });
        if bucket_collapsed {
            return;
        }
    }
    let base = if headed { 1 } else { 0 };

    for slot in slots {
        match slot {
            // A repository whose only loaded checkout is one row already: a
            // header above it would just repeat its name, and there is nothing
            // to collapse. Hoist the checkout and let it stand for the repo
            // until a second one shows up (a new worktree re-splits the group
            // on its own, since the tree is derived every render).
            Slot::Repo { ref members, .. } if members.len() == 1 => {
                let index = members[0];
                rows.push(ProjectTreeRow::Checkout {
                    bucket,
                    workspace_index: index,
                    kind: checkout_kind(&workspaces[index]),
                    depth: base,
                    hoisted: true,
                    in_group: false,
                });
            }
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
                    depth: base,
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
                        depth: base + 1,
                        hoisted: false,
                        in_group: true,
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
                        depth: base + 1,
                        hoisted: false,
                        in_group: true,
                    });
                }
            }
            Slot::Flat(index) => rows.push(ProjectTreeRow::Checkout {
                bucket,
                workspace_index: index,
                kind: checkout_kind(&workspaces[index]),
                depth: base,
                hoisted: false,
                in_group: false,
            }),
            Slot::Dir(path) => rows.push(ProjectTreeRow::Dir { bucket, path }),
        }
    }
}

/// For every collapsible row's key, the workspaces it hides when collapsed.
///
/// A collapsed header must still surface its descendants' attention signals —
/// an agent waiting for permission cannot vanish behind a chevron. The rows
/// being drawn can't answer that (a collapsed row's descendants are omitted by
/// construction), and re-deriving it from membership is exactly the kind of
/// inference the tree no longer does: a repo group holds the member checkouts
/// of that repo, not every checkout sharing its `source_repo`. So the answer
/// comes from a second pass over the FULLY EXPANDED tree, where both frontends
/// read it off the same walk.
pub fn hidden_checkouts(
    projects: &[Project],
    workspaces: &[WorkspaceInfo],
) -> HashMap<String, Vec<usize>> {
    let mut out: HashMap<String, Vec<usize>> = HashMap::new();
    // The header a row is under, as (bucket, key). Tracked by bucket, not just
    // "the last header seen": the loose repositories that follow the projects
    // have no header of their own, and would otherwise be counted under
    // whichever project happened to come last.
    let mut header: Option<(Bucket, String)> = None;
    let mut repo_key: Option<String> = None;
    for row in project_tree(projects, workspaces, &HashSet::new()) {
        if header.as_ref().is_some_and(|(b, _)| *b != row.bucket()) {
            header = None;
            repo_key = None;
        }
        match row {
            ProjectTreeRow::Project { key, bucket, .. } => {
                header = Some((bucket, key));
                repo_key = None;
            }
            ProjectTreeRow::Repo { key, .. } => repo_key = Some(key),
            ProjectTreeRow::Checkout {
                workspace_index,
                in_group,
                ..
            } => {
                if let Some((_, k)) = &header {
                    out.entry(k.clone()).or_default().push(workspace_index);
                }
                // A row that hangs off the header belongs to no repo group,
                // even though one may have been emitted just above it.
                if in_group && let Some(k) = &repo_key {
                    out.entry(k.clone()).or_default().push(workspace_index);
                }
            }
            ProjectTreeRow::Dir { .. } => {}
        }
    }
    out
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
                        hoisted,
                        ..
                    } => format!(
                        "{} {kind:?}{}",
                        workspaces[*workspace_index].name,
                        if *hoisted { "*" } else { "" }
                    ),
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
                // `lib` has a single checkout, so it is hoisted instead of
                // getting a header that repeats its name.
                ".lib Primary*",
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
                // One checkout: hoisted, so the repo costs one row, not two.
                ".api Primary*",
                // A repo member with nothing loaded still gets its row — there
                // is no checkout to hoist in its place.
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

    /// Membership is never inferred from a shared `source_repo`: putting one
    /// worktree in a project must not drag its siblings in. Which branch
    /// belongs to a piece of work is the user's call.
    #[test]
    fn a_sibling_checkout_does_not_join_the_project_by_itself() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        // Only the worktree was added.
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/feature"))],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec![
                "Project(0)",
                // The project shows the branch it was given, hoisted — one
                // checkout of that repo is in it.
                ".feature Worktree*",
                // ...and the clone stays loose, at the top level.
                "app Primary*",
            ]
        );
    }

    #[test]
    fn a_single_checkout_repo_is_hoisted_instead_of_getting_a_header() {
        let list = [clone_of("app", "/repos/app")];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(sketch(&rows, &list), vec!["Project(0)", ".app Primary*"]);
        assert!(rows[1].is_hoisted());
        assert_eq!(rows[1].depth(), 1, "it hangs off the project header");
        assert!(
            !rows
                .iter()
                .any(|r| matches!(r, ProjectTreeRow::Repo { .. })),
            "no header row at all: {rows:#?}"
        );
    }

    /// ...and the rule undoes itself the moment there is a branch tree worth
    /// showing. No stored state is involved: the tree is derived every render.
    #[test]
    fn a_second_checkout_re_splits_the_hoisted_repo() {
        let mut list = vec![clone_of("app", "/repos/app")];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        assert!(project_tree(&projects, &list, &none())[1].is_hoisted());

        // Both checkouts have to be members: nothing is inferred.
        list.push(worktree("feature", "/repos/app"));
        let projects = [project(
            1,
            "Piki",
            vec![
                ProjectMember::new(PathBuf::from("/wt/app")),
                ProjectMember::new(PathBuf::from("/wt/feature")),
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
            ]
        );
        assert!(!rows.iter().any(|r| r.is_hoisted()));
    }

    /// A lone worktree whose clone isn't loaded is still one row for one
    /// repository — hoisted, and kept honest about being a worktree.
    #[test]
    fn a_lone_worktree_is_hoisted_as_a_worktree() {
        let list = [worktree("feature", "/repos/app")];
        let rows = project_tree(&[], &list, &none());
        assert_eq!(sketch(&rows, &list), vec!["feature Worktree*"]);
    }

    /// A hoisted row has no header, so there is nothing to collapse — a stale
    /// collapse key for that repo must not make its checkout disappear.
    #[test]
    fn a_stale_collapse_key_cannot_hide_a_hoisted_checkout() {
        let list = [clone_of("app", "/repos/app")];
        let collapsed = HashSet::from([repo_collapse_key(UNASSIGNED_KEY, Path::new("/repos/app"))]);
        let rows = project_tree(&[], &list, &collapsed);
        assert_eq!(sketch(&rows, &list), vec!["app Primary*"]);
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
            vec!["Project(0)", ".app Primary*", "Project(1)", ".app Primary*"]
        );
    }

    #[test]
    fn repos_collapse_independently_per_project() {
        // Two checkouts, so the repo really has a header to collapse.
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        let members = vec![
            ProjectMember::new(PathBuf::from("/wt/app")),
            ProjectMember::new(PathBuf::from("/wt/feature")),
        ];
        let projects = [project(1, "A", members.clone()), project(2, "B", members)];
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
                "..feature Worktree",
            ]
        );
    }

    #[test]
    fn collapsing_a_project_hides_everything_under_it() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        let projects = [project(
            1,
            "Piki",
            vec![
                ProjectMember::new(PathBuf::from("/wt/app")),
                ProjectMember::new(PathBuf::from("/wt/feature")),
            ],
        )];
        let collapsed = HashSet::from(["project:1".to_string()]);
        let rows = project_tree(&projects, &list, &collapsed);
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows[0],
            ProjectTreeRow::Project {
                collapsed: true,
                ..
            }
        ));
    }

    /// A repository no project claims is NOT wrapped in a pseudo-project: it
    /// sits at the top level, after the real projects.
    #[test]
    fn loose_repos_sit_at_the_top_level_with_no_header() {
        let list = [clone_of("app", "/repos/app"), clone_of("other", "/repos/x")];
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/app"))],
        )];
        let rows = project_tree(&projects, &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec!["Project(0)", ".app Primary*", "other Primary*"]
        );
        assert_eq!(rows[2].depth(), 0, "no header to indent under");
        assert_eq!(rows[2].bucket(), Bucket::Unassigned);
        assert!(
            !rows
                .iter()
                .any(|r| matches!(r, ProjectTreeRow::Project { .. })
                    && r.bucket() == Bucket::Unassigned),
            "the no-project bucket emits no header row"
        );
    }

    /// Same at the next level down: a loose repo with two checkouts keeps its
    /// group, just one indent further out than a project's would be.
    #[test]
    fn a_loose_repo_group_sits_one_level_further_out() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        let rows = project_tree(&[], &list, &none());
        assert_eq!(
            sketch(&rows, &list),
            vec!["repo app", ".app Primary", ".feature Worktree"]
        );
        assert_eq!(rows[0].depth(), 0);
        assert_eq!(rows[1].depth(), 1);
    }

    #[test]
    fn with_no_projects_at_all_everything_is_loose() {
        let list = [clone_of("app", "/repos/app")];
        let rows = project_tree(&[], &list, &none());
        assert_eq!(sketch(&rows, &list), vec!["app Primary*"]);
    }

    #[test]
    fn an_empty_project_still_shows_its_header() {
        let projects = [project(1, "Empty", vec![])];
        let rows = project_tree(&projects, &[], &none());
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0], ProjectTreeRow::Project { .. }));
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
                ".app Primary*",
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

    /// What a collapsed header hides is its real descendants — not every
    /// checkout that happens to share a member's repository.
    #[test]
    fn hidden_checkouts_follow_membership_not_the_repo() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        // Only the worktree is a member.
        let projects = [project(
            1,
            "Piki",
            vec![ProjectMember::new(PathBuf::from("/wt/feature"))],
        )];
        let hidden = hidden_checkouts(&projects, &list);
        assert_eq!(hidden.get("project:1"), Some(&vec![1]));
        assert!(
            !hidden
                .values()
                .any(|v| v.contains(&0) && hidden.get("project:1").is_some_and(|p| p.contains(&0))),
            "the loose clone is not hidden by the project: {hidden:?}"
        );
    }

    /// A repo group answers for the checkouts actually under it, and a row
    /// hanging off the header (a PR review, a non-git workspace) belongs to no
    /// group even when one was emitted just above it.
    #[test]
    fn hidden_checkouts_scope_a_repo_group_to_its_own_children() {
        let list = [
            clone_of("app", "/repos/app"),
            worktree("feature", "/repos/app"),
            plain_dir_ws("notes"),
        ];
        let projects = [project(
            1,
            "Piki",
            vec![
                ProjectMember::new(PathBuf::from("/wt/app")),
                ProjectMember::new(PathBuf::from("/wt/feature")),
                ProjectMember::new(PathBuf::from("/wt/notes")),
            ],
        )];
        let hidden = hidden_checkouts(&projects, &list);
        let repo = repo_collapse_key("project:1", Path::new("/repos/app"));
        assert_eq!(hidden.get(&repo), Some(&vec![0, 1]));
        assert_eq!(
            hidden.get("project:1"),
            Some(&vec![0, 1, 2]),
            "the project hides the plain-directory row too"
        );
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
