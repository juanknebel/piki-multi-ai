//! Projects: user-defined cross-cutting groups of workspaces and directories.
//!
//! A project is a label with a colour, not a place: its members reference
//! workspaces (of any repo) or plain directories by path. Membership stores
//! only the path — whether a member renders as a workspace or as a directory
//! is resolved dynamically against the registered workspace list, so adopting
//! a directory as a workspace upgrades its project rows with no migration.

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
    /// Workspace path or plain directory — the member's identity.
    pub path: PathBuf,
}

impl Project {
    /// Clamp `color` into the palette range (defensive against hand-edited
    /// rows or a future palette shrink).
    pub fn clamped_color(&self) -> u8 {
        self.color.min(PROJECT_PALETTE_LEN - 1)
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
