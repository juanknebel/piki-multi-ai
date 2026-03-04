# T12 — File watcher con notify

**Status:** DONE
**Fase:** 4 — File Watching
**Bloquea:** T13, T14
**Bloqueada por:** T07

## Descripcion

Implementar un file watcher por workspace que detecte cuando Claude Code modifica
archivos en el worktree. Usa el crate `notify` con bridge a tokio channels.

## Detalle tecnico

```rust
// workspace/watcher.rs

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    rx: tokio::sync::mpsc::Receiver<WatchEvent>,
}

pub struct WatchEvent {
    pub workspace_name: String,
    pub paths: Vec<PathBuf>,
    pub kind: WatchEventKind,
}

pub enum WatchEventKind {
    Created,
    Modified,
    Deleted,
}

impl FileWatcher {
    /// Crear watcher para un directorio de worktree
    pub fn new(
        worktree_path: PathBuf,
        workspace_name: String,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let name = workspace_name.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Filtrar: ignorar .git, target/, etc.
                    let paths: Vec<_> = event.paths.iter()
                        .filter(|p| !should_ignore(p))
                        .cloned()
                        .collect();
                    if !paths.is_empty() {
                        let kind = match event.kind {
                            EventKind::Create(_) => WatchEventKind::Created,
                            EventKind::Modify(_) => WatchEventKind::Modified,
                            EventKind::Remove(_) => WatchEventKind::Deleted,
                            _ => return,
                        };
                        let _ = tx.blocking_send(WatchEvent {
                            workspace_name: name.clone(),
                            paths,
                            kind,
                        });
                    }
                }
            },
            notify::Config::default(),
        )?;

        watcher.watch(&worktree_path, RecursiveMode::Recursive)?;

        Ok(Self { _watcher: watcher, rx })
    }

    /// Recibir eventos (non-blocking)
    pub fn try_recv(&mut self) -> Option<WatchEvent> {
        self.rx.try_recv().ok()
    }
}

fn should_ignore(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/.git/") || s.contains("/target/") || s.contains(".swp")
}
```

### Debounce

Los eventos de filesystem vienen en rafagas (un `cargo build` genera cientos).
Usar un debounce de ~500ms antes de actualizar la lista de archivos.
Alternativamente, solo actualizar via `git diff --name-status` cada N segundos.

## Acceptance Criteria

- [x] Watcher detecta archivos creados, modificados, eliminados
- [x] Ignora .git/, target/, .claude/, .swp, .swo, ~, .DS_Store
- [x] Eventos se envian via tokio mpsc channel (blocking_send from notify thread)
- [x] No bloquea el main loop (try_recv + drain)
- [x] Se destruye limpiamente cuando se elimina el workspace (watcher = None in delete action)
