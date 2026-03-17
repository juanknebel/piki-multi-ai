use std::path::Path;

use crate::domain::WorkspaceInfo;
use crate::workspace::config as ws_config;
use crate::workspace::config::WorkspaceEntry;

use super::WorkspaceStorage;

pub struct JsonStorage;

impl WorkspaceStorage for JsonStorage {
    fn save_workspaces(&self, git_root: &Path, workspaces: &[WorkspaceInfo]) -> anyhow::Result<()> {
        ws_config::save(git_root, workspaces)
    }

    fn load_workspaces(&self, git_root: &Path) -> anyhow::Result<Vec<WorkspaceEntry>> {
        ws_config::load(git_root)
    }

    fn load_all_workspaces(&self) -> Vec<WorkspaceEntry> {
        ws_config::load_all()
    }
}
