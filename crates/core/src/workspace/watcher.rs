use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Kind of filesystem change detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEventKind {
    Created,
    Modified,
    Deleted,
}

/// A filesystem event from a watched worktree
#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub workspace_name: String,
    pub paths: Vec<PathBuf>,
    pub kind: WatchEventKind,
}

/// Watches a worktree directory for file changes using the notify crate.
/// Events are sent to a tokio mpsc channel for async consumption.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    rx: tokio::sync::mpsc::Receiver<WatchEvent>,
}

impl FileWatcher {
    /// Create a new file watcher for the given worktree directory.
    /// Watches recursively, filtering out .git/, target/, and temp files.
    pub fn new(worktree_path: PathBuf, workspace_name: String) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let name_for_log = workspace_name.clone();
        let name = workspace_name;

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Filter out ignored paths
                    let paths: Vec<_> = event
                        .paths
                        .iter()
                        .filter(|p| !should_ignore(p))
                        .cloned()
                        .collect();

                    if paths.is_empty() {
                        return;
                    }

                    let kind = match event.kind {
                        EventKind::Create(_) => WatchEventKind::Created,
                        EventKind::Modify(_) => WatchEventKind::Modified,
                        EventKind::Remove(_) => WatchEventKind::Deleted,
                        _ => return,
                    };

                    // blocking_send is fine here — called from notify's background thread
                    let _ = tx.blocking_send(WatchEvent {
                        workspace_name: name.clone(),
                        paths,
                        kind,
                    });
                }
            },
            notify::Config::default(),
        )?;

        watcher.watch(&worktree_path, RecursiveMode::Recursive)?;

        tracing::info!(workspace = %name_for_log, path = %worktree_path.display(), "file watcher started");

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Try to receive the next event without blocking.
    pub fn try_recv(&mut self) -> Option<WatchEvent> {
        self.rx.try_recv().ok()
    }

    /// Drain all pending events, returning them as a Vec.
    pub fn drain(&mut self) -> Vec<WatchEvent> {
        let mut events = Vec::new();
        while let Some(ev) = self.try_recv() {
            events.push(ev);
        }
        events
    }
}

/// Returns true for paths that should be ignored by the watcher.
fn should_ignore(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/.git/")
        || s.contains("/target/")
        || s.contains("/.claude/")
        || s.ends_with(".swp")
        || s.ends_with(".swo")
        || s.ends_with('~')
        || s.contains(".DS_Store")
}
