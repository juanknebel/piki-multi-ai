use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Upper bound on how many directories `register_new_dirs` will register (and
/// walk one level into) per call. Each registration is a blocking round-trip
/// to notify's background inotify thread; without a cap, a single burst of
/// directory creation (e.g. `git clone`, a big worktree add, a build tool
/// materializing a tree) could queue thousands of them and block the main
/// event-loop thread — and therefore all keyboard input — until the whole
/// backlog drained. Capping it spreads the work over subsequent ticks
/// instead.
const MAX_WATCHES_PER_TICK: usize = 64;

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
    watcher: RecommendedWatcher,
    rx: tokio::sync::mpsc::Receiver<WatchEvent>,
    /// Selective (Linux) mode only: directories created after startup,
    /// queued by the event callback for `try_recv`/`drain` to register
    /// watches on. `None` when the root watch is recursive.
    new_dirs: Option<std::sync::mpsc::Receiver<PathBuf>>,
    /// Selective mode only: directories still waiting to be registered,
    /// carried over between calls when a burst exceeds `MAX_WATCHES_PER_TICK`.
    pending: VecDeque<PathBuf>,
}

impl FileWatcher {
    /// Create a new file watcher for the given worktree directory.
    ///
    /// On Linux (inotify) `RecursiveMode::Recursive` makes the notify crate
    /// register a kernel watch for EVERY directory in the tree — including
    /// `target/`, `node_modules/` and `.git/`, whose event storms would then
    /// be delivered to userspace only to be dropped by `should_ignore`. With
    /// many workspaces open that means tens of thousands of kernel watches
    /// per project and constant churn during builds. Instead we walk the
    /// tree ourselves, skip ignored directories entirely, and register one
    /// non-recursive watch per remaining directory. Directories created
    /// later are picked up from the event stream and registered on the next
    /// `try_recv`/`drain`.
    ///
    /// On macOS/Windows the recursive watch is a single cheap kernel-side
    /// subscription (FSEvents / ReadDirectoryChangesW), so it stays
    /// recursive there and relies on the event-side filter alone.
    pub fn new(worktree_path: PathBuf, workspace_name: String) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let (dir_tx, dir_rx) = std::sync::mpsc::channel::<PathBuf>();
        let name_for_log = workspace_name.clone();
        let name = workspace_name;
        let selective = cfg!(target_os = "linux");

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

                    // Selective mode: queue newly created directories so the
                    // consumer can extend the per-directory watch set.
                    if selective && kind == WatchEventKind::Created {
                        for p in &paths {
                            let ignored_name = p
                                .file_name()
                                .is_some_and(|n| is_ignored_dir(&n.to_string_lossy()));
                            if !ignored_name && p.is_dir() {
                                let _ = dir_tx.send(p.clone());
                            }
                        }
                    }

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

        let new_dirs = if selective {
            let watched = watch_tree(&mut watcher, &worktree_path, true)?;
            tracing::info!(
                workspace = %name_for_log,
                path = %worktree_path.display(),
                dirs = watched,
                "file watcher started (selective)"
            );
            Some(dir_rx)
        } else {
            watcher.watch(&worktree_path, RecursiveMode::Recursive)?;
            tracing::info!(
                workspace = %name_for_log,
                path = %worktree_path.display(),
                "file watcher started (recursive)"
            );
            None
        };

        Ok(Self {
            watcher,
            rx,
            new_dirs,
            pending: VecDeque::new(),
        })
    }

    /// Register watches for directories created after startup (queued by the
    /// event callback), plus any left over from a previous call. Selective
    /// (Linux) mode only; no-op otherwise. Bounded by `MAX_WATCHES_PER_TICK`
    /// so a large burst is spread across calls instead of blocking the
    /// caller (the main event-loop thread) until it fully drains.
    fn register_new_dirs(&mut self) {
        let Some(ref dir_rx) = self.new_dirs else {
            return;
        };
        self.pending
            .extend(std::iter::from_fn(|| dir_rx.try_recv().ok()));

        for _ in 0..MAX_WATCHES_PER_TICK {
            let Some(dir) = self.pending.pop_front() else {
                break;
            };
            // Best-effort: the directory may already be gone, or a watch may
            // already exist (re-adding is fine for inotify). One level only
            // per directory here — deeper subdirectories are queued onto
            // `pending` rather than walked recursively in this call.
            let Ok(()) = self.watcher.watch(&dir, RecursiveMode::NonRecursive) else {
                continue;
            };
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                if !ft.is_dir() {
                    continue;
                }
                if is_ignored_dir(&entry.file_name().to_string_lossy()) {
                    continue;
                }
                self.pending.push_back(entry.path());
            }
        }

        if !self.pending.is_empty() {
            tracing::debug!(
                remaining = self.pending.len(),
                "watch registration backlog continues next tick"
            );
        }
    }

    /// Try to receive the next event without blocking.
    pub fn try_recv(&mut self) -> Option<WatchEvent> {
        self.register_new_dirs();
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

/// Walk `root` and register one non-recursive watch per directory, skipping
/// ignored directories and symlinks. Returns the number of directories
/// watched. With `strict_root`, failure to watch `root` itself is an error
/// (startup path); child failures (e.g. hitting the inotify watch limit) are
/// logged and skipped.
fn watch_tree(
    watcher: &mut RecommendedWatcher,
    root: &Path,
    strict_root: bool,
) -> anyhow::Result<usize> {
    let mut watched = 0usize;
    let mut failures = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        match watcher.watch(&dir, RecursiveMode::NonRecursive) {
            Ok(()) => watched += 1,
            Err(e) if watched == 0 && failures == 0 && strict_root => return Err(e.into()),
            Err(_) => {
                failures += 1;
                continue;
            }
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // `file_type` does not follow symlinks, so symlinked dirs are
            // excluded (watching through them could escape the worktree).
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            if is_ignored_dir(&entry.file_name().to_string_lossy()) {
                continue;
            }
            stack.push(entry.path());
        }
    }
    if failures > 0 {
        tracing::warn!(
            root = %root.display(),
            failures,
            "some directories could not be watched (inotify watch limit?)"
        );
    }
    Ok(watched)
}

/// Directory names never worth watching — build outputs and VCS/tool
/// internals whose event volume dwarfs their signal. Must stay a superset of
/// the directory names filtered by `should_ignore`, since in selective mode
/// these subtrees are never registered at all.
fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".claude" | ".venv" | "dist" | "build"
    )
}

/// Returns true for paths that should be ignored by the watcher.
///
/// The `contains` checks match paths *inside* an ignored directory; the
/// `file_name` check matches an event about the ignored directory itself
/// (e.g. macOS FSEvents reports `.../node_modules` with no trailing slash
/// when the directory is created or coalesces events at dir granularity).
fn should_ignore(path: &Path) -> bool {
    if path
        .file_name()
        .is_some_and(|n| is_ignored_dir(&n.to_string_lossy()))
    {
        return true;
    }
    let s = path.to_string_lossy();
    s.contains("/.git/")
        || s.contains("/target/")
        || s.contains("/node_modules/")
        || s.contains("/.claude/")
        || s.ends_with(".swp")
        || s.ends_with(".swo")
        || s.ends_with('~')
        || s.contains(".DS_Store")
}
