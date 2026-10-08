//! Projects: the one top-level unit the sidebar navigates.
//!
//! A project is a label with a colour, not a place: its members reference
//! repositories, workspaces (of any repo) or plain directories by path. The
//! worktrees a project shows still live under `<data_dir>/worktrees/<repo>/`
//! — the project is a *view*, never the owner of the layout on disk, which is
//! what lets one repo belong to several projects.
//!
//! Membership stores only the path plus a [`MemberKind`] hint. `Auto` members
//! are resolved dynamically against the registered workspace list, so
//! adopting a directory as a workspace upgrades its project rows with no
//! migration; `Repo` members are the explicit "this repository belongs to the
//! project" rows that must render even when no checkout of them is loaded.
//!
//! [`tree::project_tree`] turns a project list plus the live workspace list
//! into the three-level sidebar model (project → repo → checkout). Both
//! frontends render *only* that — there is no separate flat workspace view.

pub mod tree;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Number of entries in the fixed project colour palette. `Project.color` is
/// always an index into it — never a colour value — so both frontends (and
/// any theme) decide what the ten slots look like.
pub const PROJECT_PALETTE_LEN: u8 = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    /// SQLite rowid; `None` until first saved.
    pub id: Option<i64>,
    /// Unique display name.
    pub name: String,
    /// Palette index, `0..PROJECT_PALETTE_LEN` (clamped on load).
    pub color: u8,
    /// Persistent display order (lower first), assigned max+1 on create.
    pub order: u32,
    /// Ordered members; position is the index in this vec.
    pub members: Vec<ProjectMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMember {
    /// Workspace path, repository root or plain directory — the member's
    /// identity. The only thing that identifies a member; two members of the
    /// same project can never share a path.
    pub path: PathBuf,
    /// How to render the member. See [`MemberKind`].
    #[serde(default)]
    pub kind: MemberKind,
}

/// How a [`ProjectMember`] renders in the tree.
///
/// Only the explicit `Repo` case needs storing: everything else is resolved
/// against the live workspace list at render time, which is what makes
/// adopting a directory as a workspace a zero-migration upgrade.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemberKind {
    /// Resolve dynamically: a registered workspace at this path renders as a
    /// checkout (nested under its `source_repo`), anything else as a plain
    /// directory row.
    #[default]
    Auto,
    /// The user added this path as a repository root. It heads a repo group
    /// even when none of its checkouts are loaded, so a project can list a
    /// repo it hasn't opened yet.
    Repo,
}

impl MemberKind {
    /// SQL `kind` column value.
    pub fn tag(self) -> &'static str {
        match self {
            MemberKind::Auto => "auto",
            MemberKind::Repo => "repo",
        }
    }

    /// Rebuild from the SQL `kind` column. Unknown tags fall back to `Auto`,
    /// which only ever costs a row its repo header, never its identity.
    pub fn from_sql(tag: &str) -> Self {
        match tag {
            "repo" => MemberKind::Repo,
            _ => MemberKind::Auto,
        }
    }
}

impl ProjectMember {
    /// An `Auto` member — the common case.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            kind: MemberKind::Auto,
        }
    }

    /// An explicit repository-root member.
    pub fn repo(path: PathBuf) -> Self {
        Self {
            path,
            kind: MemberKind::Repo,
        }
    }
}

impl Project {
    /// Clamp `color` into the palette range (defensive against hand-edited
    /// rows or a future palette shrink).
    pub fn clamped_color(&self) -> u8 {
        self.color.min(PROJECT_PALETTE_LEN - 1)
    }

    /// Stable collapse/expansion key for this project's header row. Falls
    /// back to the name for a project that was never saved (`id == None`),
    /// which only happens while a creation dialog is still open.
    pub fn collapse_key(&self) -> String {
        match self.id {
            Some(id) => format!("project:{id}"),
            None => format!("project:name:{}", self.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_is_clamped_to_palette() {
        let mut p = Project {
            id: None,
            name: "x".into(),
            color: 9,
            order: 0,
            members: Vec::new(),
        };
        assert_eq!(p.clamped_color(), 9);
        p.color = 200;
        assert_eq!(p.clamped_color(), PROJECT_PALETTE_LEN - 1);
    }
}
