use std::collections::HashSet;
use std::path::Path;

use crate::domain::WorkspaceInfo;
use crate::workspace::config::WorkspaceEntry;

pub mod json;
pub mod sqlite;

/// Entry representing an API request/response pair in history
pub struct ApiHistoryEntry {
    pub id: Option<i64>,
    pub source_repo: String,
    pub created_at: String,
    pub request_text: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub elapsed_ms: u128,
    pub response_body: String,
    pub response_headers: String,
}

pub trait WorkspaceStorage: Send + Sync {
    fn save_workspaces(&self, git_root: &Path, workspaces: &[WorkspaceInfo]) -> anyhow::Result<()>;
    fn load_workspaces(&self, git_root: &Path) -> anyhow::Result<Vec<WorkspaceEntry>>;
    fn load_all_workspaces(&self) -> Vec<WorkspaceEntry>;
}

pub trait ApiHistoryStorage: Send + Sync {
    fn save_api_entry(&self, entry: &ApiHistoryEntry) -> anyhow::Result<()>;
    fn search_api_history(
        &self,
        source_repo: &Path,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<ApiHistoryEntry>>;
    fn load_recent_api_history(
        &self,
        source_repo: &Path,
        limit: usize,
    ) -> anyhow::Result<Vec<ApiHistoryEntry>>;
    fn delete_api_entry(&self, id: i64) -> anyhow::Result<()>;
}

pub trait UiPrefsStorage: Send + Sync {
    fn get_collapsed_groups(&self) -> anyhow::Result<HashSet<String>>;
    fn set_collapsed_groups(&self, groups: &HashSet<String>) -> anyhow::Result<()>;
    fn get_preference(&self, key: &str) -> anyhow::Result<Option<String>>;
    fn set_preference(&self, key: &str, value: &str) -> anyhow::Result<()>;
}

pub struct AppStorage {
    pub workspaces: Box<dyn WorkspaceStorage>,
    pub api_history: Option<Box<dyn ApiHistoryStorage>>,
    pub ui_prefs: Option<Box<dyn UiPrefsStorage>>,
}

pub fn create_storage() -> anyhow::Result<AppStorage> {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("piki-multi");
    let db_path = data_dir.join("piki.db");
    std::fs::create_dir_all(db_path.parent().unwrap())?;
    let store = std::sync::Arc::new(sqlite::SqliteStorage::open(&db_path)?);
    Ok(AppStorage {
        workspaces: Box::new(std::sync::Arc::clone(&store)),
        api_history: Some(Box::new(std::sync::Arc::clone(&store))),
        ui_prefs: Some(Box::new(store)),
    })
}
