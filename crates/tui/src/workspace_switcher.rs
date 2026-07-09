use crate::app::Workspace;

/// A workspace and its tabs, captured when the switcher opens. Names are
/// cached for filtering; live status is derived at render time from
/// `App::workspaces` (so glyphs stay fresh).
pub struct WsNode {
    pub ws_idx: usize,
    pub name: String,
    pub tabs: Vec<TabNode>,
}

pub struct TabNode {
    pub tab_idx: usize,
    pub label: String,
}

/// A single visible row of the switcher tree — either a workspace header or
/// one of its tabs. `selected` in the state indexes into the filtered `rows`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitcherRow {
    Workspace { ws_idx: usize },
    Tab { ws_idx: usize, tab_idx: usize },
}

/// State for the tree-style workspace switcher overlay.
pub struct WorkspaceSwitcherState {
    pub query: String,
    /// The full tree, captured on open.
    tree: Vec<WsNode>,
    /// Flattened, filtered rows currently shown (recomputed on query change).
    pub rows: Vec<SwitcherRow>,
    pub selected: usize,
}

impl WorkspaceSwitcherState {
    /// The row under the cursor, if any.
    pub fn selected_row(&self) -> Option<SwitcherRow> {
        self.rows.get(self.selected).copied()
    }

    /// Total workspaces captured (for the header counter).
    pub fn workspace_count(&self) -> usize {
        self.tree.len()
    }

    /// Recompute `rows` for the current query and clamp the selection.
    pub fn refilter(&mut self) {
        self.rows = build_rows(&self.tree, &self.query);
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
    }
}

/// Create the switcher, capturing every workspace and its tabs as a tree.
pub fn create_state(workspaces: &[Workspace]) -> WorkspaceSwitcherState {
    let tree: Vec<WsNode> = workspaces
        .iter()
        .enumerate()
        .map(|(ws_idx, ws)| WsNode {
            ws_idx,
            name: ws.name.clone(),
            tabs: ws
                .tabs
                .iter()
                .enumerate()
                .map(|(tab_idx, tab)| TabNode {
                    tab_idx,
                    label: tab
                        .markdown_label
                        .as_deref()
                        .unwrap_or(tab.provider.label())
                        .to_string(),
                })
                .collect(),
        })
        .collect();

    let rows = build_rows(&tree, "");
    WorkspaceSwitcherState {
        query: String::new(),
        tree,
        rows,
        selected: 0,
    }
}

/// Flatten the tree into visible rows, applying a case-insensitive substring
/// filter. A workspace whose name matches shows all its tabs; otherwise only
/// its matching tabs show (with the workspace kept as a header). An empty
/// query shows everything. Pure — unit-tested below.
fn build_rows(tree: &[WsNode], query: &str) -> Vec<SwitcherRow> {
    let q = query.trim().to_lowercase();
    let mut rows = Vec::new();
    for node in tree {
        let ws_match = q.is_empty() || node.name.to_lowercase().contains(&q);
        let matching_tabs: Vec<&TabNode> = if ws_match {
            node.tabs.iter().collect()
        } else {
            node.tabs
                .iter()
                .filter(|t| t.label.to_lowercase().contains(&q))
                .collect()
        };
        // Skip a workspace entirely when nothing under it matches.
        if !ws_match && matching_tabs.is_empty() {
            continue;
        }
        rows.push(SwitcherRow::Workspace { ws_idx: node.ws_idx });
        for t in matching_tabs {
            rows.push(SwitcherRow::Tab {
                ws_idx: node.ws_idx,
                tab_idx: t.tab_idx,
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<WsNode> {
        vec![
            WsNode {
                ws_idx: 0,
                name: "piki-nightly".into(),
                tabs: vec![
                    TabNode { tab_idx: 0, label: "Shell".into() },
                    TabNode { tab_idx: 1, label: "Claude".into() },
                ],
            },
            WsNode {
                ws_idx: 1,
                name: "bob-the-builder".into(),
                tabs: vec![TabNode { tab_idx: 0, label: "Claude".into() }],
            },
        ]
    }

    #[test]
    fn empty_query_shows_full_tree() {
        let rows = build_rows(&tree(), "");
        assert_eq!(
            rows,
            vec![
                SwitcherRow::Workspace { ws_idx: 0 },
                SwitcherRow::Tab { ws_idx: 0, tab_idx: 0 },
                SwitcherRow::Tab { ws_idx: 0, tab_idx: 1 },
                SwitcherRow::Workspace { ws_idx: 1 },
                SwitcherRow::Tab { ws_idx: 1, tab_idx: 0 },
            ]
        );
    }

    #[test]
    fn workspace_name_match_includes_all_its_tabs() {
        let rows = build_rows(&tree(), "nightly");
        assert_eq!(
            rows,
            vec![
                SwitcherRow::Workspace { ws_idx: 0 },
                SwitcherRow::Tab { ws_idx: 0, tab_idx: 0 },
                SwitcherRow::Tab { ws_idx: 0, tab_idx: 1 },
            ]
        );
    }

    #[test]
    fn tab_match_keeps_workspace_header_and_only_matching_tabs() {
        // "claude" doesn't match either workspace name, but matches one tab in
        // each — each workspace is kept as a header with just that tab.
        let rows = build_rows(&tree(), "claude");
        assert_eq!(
            rows,
            vec![
                SwitcherRow::Workspace { ws_idx: 0 },
                SwitcherRow::Tab { ws_idx: 0, tab_idx: 1 },
                SwitcherRow::Workspace { ws_idx: 1 },
                SwitcherRow::Tab { ws_idx: 1, tab_idx: 0 },
            ]
        );
    }

    #[test]
    fn no_match_yields_no_rows() {
        assert!(build_rows(&tree(), "zzz").is_empty());
    }

    #[test]
    fn filtering_is_case_insensitive() {
        assert_eq!(build_rows(&tree(), "PIKI").len(), 3);
    }
}
