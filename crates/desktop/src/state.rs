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
    pub provider_manager: piki_core::providers::ProviderManager,
    /// Handle to the persistent-session daemon, when reachable. `None` means
    /// sessions are disabled/unavailable — tabs then run in-process (Local).
    pub session_daemon: Option<piki_core::session::client::Daemon>,
    /// Effective `[sessions] enabled` this process started with (precedence:
    /// DB override, then config.toml, then the default — see
    /// `piki_core::app_settings`). A change made in Settings ▸ General applies
    /// on the next launch; `session_status` reports both so the status bar
    /// can say "restart".
    pub sessions_enabled: bool,
    /// What startup re-attach restored; read once by the frontend.
    pub restore_summary: crate::session::RestoreSummary,
    /// Global AI chat messages (not tied to any workspace).
    pub chat_messages: Vec<piki_core::chat::ChatMessage>,
    /// Global AI chat configuration (provider, model, base URL).
    pub chat_config: piki_core::chat::ChatConfig,
    /// Whether a chat response is currently being streamed.
    pub chat_streaming: bool,
    /// Whether agent mode (tool-use) is enabled for chat.
    pub chat_agent_mode: bool,
}

#[allow(dead_code)]
pub struct DesktopWorkspace {
    pub info: WorkspaceInfo,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
    /// Current git branch, refreshed alongside `ahead_behind`. `None` until
    /// the first refresh completes, or if the workspace isn't a git repo.
    pub branch: Option<String>,
    pub tabs: Vec<DesktopTab>,
    pub active_tab: usize,
    pub watcher: Option<FileWatcher>,
    /// Memoised `piki_core::search::list_files` result behind `Ctrl+F`.
    /// Filled off-lock by `commands::search::fuzzy_file_list`; cleared by
    /// `events::spawn_git_watcher` when the watcher reports anything other
    /// than an edit to an already-indexed file, and by `switch_workspace`.
    pub file_index: Option<Arc<piki_core::search::FileIndex>>,
}

pub struct DesktopTab {
    pub id: String,
    pub provider: AIProvider,
    pub pty: Option<RawPtySession>,
    pub alive: bool,
    /// Custom title set by user (takes precedence over provider label).
    pub custom_title: Option<String>,
    /// Idle watcher for provider tabs (`AIProvider::Custom(_)`). Polled by
    /// the background tick loop in `main.rs`. `None` for Shell, Kanban, etc.
    pub idle_watcher: Option<piki_core::idle_watcher::IdleWatcher>,
}

impl DesktopTab {
    /// `provider_cfg` is the matching `providers.toml` entry for `Custom`
    /// providers; its per-provider idle knobs (`idle_threshold_secs` /
    /// `idle_notify`) drive the tab's `IdleWatcher`. `None` for built-ins.
    pub fn new(
        provider: AIProvider,
        provider_cfg: Option<&piki_core::providers::ProviderConfig>,
    ) -> Self {
        let idle_watcher = matches!(provider, AIProvider::Custom(_))
            .then(|| piki_core::idle_watcher::IdleWatcher::from_provider_config(provider_cfg));
        Self {
            id: Uuid::new_v4().to_string(),
            provider,
            pty: None,
            alive: false,
            custom_title: None,
            idle_watcher,
        }
    }

    pub fn display_label(&self) -> String {
        if let Some(custom) = self.custom_title.as_deref()
            && !custom.trim().is_empty()
        {
            return custom.to_string();
        }
        self.provider.label().to_string()
    }
}

#[derive(Serialize, Clone)]
pub struct TabInfo {
    pub id: String,
    pub provider: AIProvider,
    pub alive: bool,
    pub custom_title: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct WorkspaceDetail {
    pub info: WorkspaceInfo,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
    pub branch: Option<String>,
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
            branch: self.branch.clone(),
            tabs: self
                .tabs
                .iter()
                .map(|t| TabInfo {
                    id: t.id.clone(),
                    provider: t.provider.clone(),
                    alive: t.alive,
                    custom_title: t.custom_title.clone(),
                })
                .collect(),
            active_tab: self.active_tab,
        }
    }
}
