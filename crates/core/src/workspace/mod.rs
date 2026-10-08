pub mod config;
pub mod manager;
pub mod watcher;

pub use manager::{ExistingWorktree, WorkspaceManager};
pub use watcher::FileWatcher;
