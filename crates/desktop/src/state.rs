use std::sync::Arc;

use parking_lot::Mutex as PlMutex;
use serde::Serialize;
use uuid::Uuid;

use piki_core::paths::DataPaths;
use piki_core::storage::AppStorage;
use piki_core::workspace::manager::WorkspaceManager;
use piki_core::workspace::watcher::FileWatcher;
use piki_core::{AIProvider, ChangedFile, WorkspaceInfo, WorkspaceStatus};

use crate::pty_raw::RawPtySession;

#[allow(dead_code)]
pub struct DesktopApp {
    pub workspaces: Vec<DesktopWorkspace>,
    pub active_workspace: usize,
    pub storage: Arc<AppStorage>,
    pub paths: DataPaths,
    pub manager: WorkspaceManager,
    pub sysinfo: Arc<PlMutex<String>>,
}

#[allow(dead_code)]
pub struct DesktopWorkspace {
    pub info: WorkspaceInfo,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
    pub tabs: Vec<DesktopTab>,
    pub active_tab: usize,
    pub watcher: Option<FileWatcher>,
}

pub struct DesktopTab {
    pub id: String,
    pub provider: AIProvider,
    pub pty: Option<RawPtySession>,
    pub alive: bool,
}

impl DesktopTab {
    pub fn new(provider: AIProvider) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            provider,
            pty: None,
            alive: false,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct TabInfo {
    pub id: String,
    pub provider: AIProvider,
    pub alive: bool,
}

#[derive(Serialize, Clone)]
pub struct WorkspaceDetail {
    pub info: WorkspaceInfo,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
    pub tabs: Vec<TabInfo>,
    pub active_tab: usize,
}

impl DesktopWorkspace {
    pub fn to_detail(&self) -> WorkspaceDetail {
        WorkspaceDetail {
            info: self.info.clone(),
            status: self.status.clone(),
            changed_files: self.changed_files.clone(),
            ahead_behind: self.ahead_behind,
            tabs: self
                .tabs
                .iter()
                .map(|t| TabInfo {
                    id: t.id.clone(),
                    provider: t.provider,
                    alive: t.alive,
                })
                .collect(),
            active_tab: self.active_tab,
        }
    }
}
