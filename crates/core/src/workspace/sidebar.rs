//! Grouping rules for the workspace sidebar, shared by both frontends.
//!
//! Workspaces sharing a `source_repo` form a **family**: the member that
//! isn't a worktree is the parent (it's the original clone), its worktree
//! siblings are children shown underneath it. PR-review workspaces are
//! `ephemeral` and each is its own ad-hoc checkout, so they never share a
//! `source_repo` and the family rule can't group them — they get one
//! synthetic header of their own, emitted first.
//!
//! This lived in the TUI (`App::sidebar_items`) and was separately re-coded
//! in the desktop's TypeScript, which never learned about the ephemeral
//! group, so PR-review workspaces rendered ungrouped there. The TUI also
//! re-derived "is this a child?" a second time at render. One function now
//! answers all of it.

use std::collections::HashSet;

use crate::domain::{WorkspaceInfo, WorkspaceType};

/// Collapse key for the synthetic PR-review group. Not a real `source_repo`,
/// so it can't collide with a family key.
pub const PR_REVIEW_GROUP_KEY: &str = "pr-review";

/// Collapse key for a family, derived from the shared `source_repo`.
pub fn family_key(info: &WorkspaceInfo) -> String {
    info.source_repo.to_string_lossy().to_string()
}

/// What a workspace row is within its family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// No family, or a family whose parent isn't loaded: renders flat.
    Standalone,
    /// Heads a family; can be collapsed to hide its children.
    Parent { key: String, collapsed: bool },
    /// A worktree sibling shown under its parent.
    Child,
}

/// One visual row, in render order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRow {
    /// Synthetic header for the PR-review group.
    PrReviewHeader { collapsed: bool },
    /// A real workspace, indexed into the slice passed to [`sidebar_rows`].
    Workspace { index: usize, kind: RowKind },
}

/// Build the sidebar's visual rows.
///
/// `collapsed` holds the currently-collapsed group keys — family keys from
/// [`family_key`] plus possibly [`PR_REVIEW_GROUP_KEY`]. Rows hidden by a
/// collapsed group are omitted entirely, so the result is exactly what should
/// be drawn, top to bottom.
pub fn sidebar_rows(workspaces: &[WorkspaceInfo], collapsed: &HashSet<String>) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    let mut consumed = vec![false; workspaces.len()];

    // PR-review workspaces first, under their own header.
    let review: Vec<usize> = workspaces
        .iter()
        .enumerate()
        .filter(|(_, w)| w.ephemeral)
        .map(|(i, _)| i)
        .collect();
    if !review.is_empty() {
        let is_collapsed = collapsed.contains(PR_REVIEW_GROUP_KEY);
        rows.push(SidebarRow::PrReviewHeader {
            collapsed: is_collapsed,
        });
        for idx in review {
            consumed[idx] = true;
            if !is_collapsed {
                rows.push(SidebarRow::Workspace {
                    index: idx,
                    kind: RowKind::Standalone,
                });
            }
        }
    }

    for i in 0..workspaces.len() {
        if consumed[i] {
            continue;
        }
        let source_repo = &workspaces[i].source_repo;
        let siblings: Vec<usize> = workspaces
            .iter()
            .enumerate()
            .filter(|(j, w)| !consumed[*j] && &w.source_repo == source_repo)
            .map(|(j, _)| j)
            .collect();

        if siblings.len() <= 1 {
            consumed[i] = true;
            rows.push(SidebarRow::Workspace {
                index: i,
                kind: RowKind::Standalone,
            });
            continue;
        }

        let parent = siblings
            .iter()
            .copied()
            .find(|&idx| workspaces[idx].workspace_type != WorkspaceType::Worktree);

        match parent {
            Some(parent_idx) => {
                let key = family_key(&workspaces[parent_idx]);
                let is_collapsed = collapsed.contains(&key);
                rows.push(SidebarRow::Workspace {
                    index: parent_idx,
                    kind: RowKind::Parent {
                        key,
                        collapsed: is_collapsed,
                    },
                });
                for &idx in &siblings {
                    consumed[idx] = true;
                    if idx != parent_idx && !is_collapsed {
                        rows.push(SidebarRow::Workspace {
                            index: idx,
                            kind: RowKind::Child,
                        });
                    }
                }
            }
            // Worktrees whose original clone isn't loaded: nothing to nest
            // under, so they render flat rather than orphaned under a
            // header that doesn't exist.
            None => {
                for &idx in &siblings {
                    consumed[idx] = true;
                    rows.push(SidebarRow::Workspace {
                        index: idx,
                        kind: RowKind::Standalone,
                    });
                }
            }
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws(name: &str, repo: &str, kind: WorkspaceType, ephemeral: bool) -> WorkspaceInfo {
        WorkspaceInfo {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{name}")),
            workspace_type: kind,
            description: String::new(),
            prompt: String::new(),
            kanban_path: None,
            order: 0,
            source_repo: PathBuf::from(repo),
            source_repo_display: String::new(),
            dispatch_card_id: None,
            dispatch_source_kanban: None,
            dispatch_agent_name: None,
            origin: Default::default(),
            is_git_repo: true,
            ephemeral,
            pr_repo_nwo: None,
            pr_number: None,
        }
    }

    fn simple(name: &str, repo: &str) -> WorkspaceInfo {
        ws(name, repo, WorkspaceType::Simple, false)
    }

    fn worktree(name: &str, repo: &str) -> WorkspaceInfo {
        ws(name, repo, WorkspaceType::Worktree, false)
    }

    fn review(name: &str) -> WorkspaceInfo {
        ws(name, &format!("/repos/{name}"), WorkspaceType::Simple, true)
    }

    fn none() -> HashSet<String> {
        HashSet::new()
    }

    fn indices(rows: &[SidebarRow]) -> Vec<usize> {
        rows.iter()
            .filter_map(|r| match r {
                SidebarRow::Workspace { index, .. } => Some(*index),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn unrelated_workspaces_render_flat() {
        let list = [simple("a", "/repos/a"), simple("b", "/repos/b")];
        let rows = sidebar_rows(&list, &none());
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| matches!(
            r,
            SidebarRow::Workspace {
                kind: RowKind::Standalone,
                ..
            }
        )));
    }

    #[test]
    fn a_family_puts_the_clone_first_and_nests_its_worktrees() {
        // Declared out of order to prove the parent is chosen by type, not
        // by position.
        let list = [
            worktree("feature", "/repos/app"),
            simple("app", "/repos/app"),
            worktree("bugfix", "/repos/app"),
        ];
        let rows = sidebar_rows(&list, &none());
        assert_eq!(indices(&rows), vec![1, 0, 2], "parent leads its children");
        assert!(matches!(
            rows[0],
            SidebarRow::Workspace {
                kind: RowKind::Parent {
                    collapsed: false,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            rows[1],
            SidebarRow::Workspace {
                kind: RowKind::Child,
                ..
            }
        ));
    }

    #[test]
    fn collapsing_a_family_hides_the_children_but_keeps_the_parent() {
        let list = [
            simple("app", "/repos/app"),
            worktree("feature", "/repos/app"),
        ];
        let collapsed = HashSet::from(["/repos/app".to_string()]);
        let rows = sidebar_rows(&list, &collapsed);
        assert_eq!(indices(&rows), vec![0]);
        assert!(matches!(
            rows[0],
            SidebarRow::Workspace {
                kind: RowKind::Parent {
                    collapsed: true,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn worktrees_without_their_clone_render_flat_not_orphaned() {
        let list = [worktree("a", "/repos/app"), worktree("b", "/repos/app")];
        let rows = sidebar_rows(&list, &none());
        assert_eq!(indices(&rows), vec![0, 1]);
        assert!(rows.iter().all(|r| matches!(
            r,
            SidebarRow::Workspace {
                kind: RowKind::Standalone,
                ..
            }
        )));
    }

    /// The case the desktop's TypeScript copy never implemented.
    #[test]
    fn pr_review_workspaces_group_under_one_header_first() {
        let list = [simple("app", "/repos/app"), review("pr-1"), review("pr-2")];
        let rows = sidebar_rows(&list, &none());
        assert!(
            matches!(rows[0], SidebarRow::PrReviewHeader { collapsed: false }),
            "the review header leads the sidebar"
        );
        assert_eq!(indices(&rows), vec![1, 2, 0]);
    }

    #[test]
    fn collapsing_the_review_group_hides_its_members() {
        let list = [simple("app", "/repos/app"), review("pr-1")];
        let collapsed = HashSet::from([PR_REVIEW_GROUP_KEY.to_string()]);
        let rows = sidebar_rows(&list, &collapsed);
        assert!(matches!(
            rows[0],
            SidebarRow::PrReviewHeader { collapsed: true }
        ));
        assert_eq!(indices(&rows), vec![0], "only the non-review workspace");
    }

    /// An ephemeral workspace is claimed by the review group even when it
    /// shares a source_repo with a normal one, so it can't appear twice.
    #[test]
    fn review_workspaces_are_never_also_grouped_into_a_family() {
        let list = [
            simple("app", "/repos/app"),
            ws("pr-9", "/repos/app", WorkspaceType::Simple, true),
        ];
        let rows = sidebar_rows(&list, &none());
        let idx = indices(&rows);
        assert_eq!(
            idx.len(),
            2,
            "each workspace appears exactly once: {rows:?}"
        );
        assert_eq!(idx, vec![1, 0]);
    }

    #[test]
    fn empty_input_produces_no_rows() {
        assert!(sidebar_rows(&[], &none()).is_empty());
    }
}
