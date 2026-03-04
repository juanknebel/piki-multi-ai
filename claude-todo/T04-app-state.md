# T04 — App state struct

**Status:** DONE
**Fase:** 1 — Core App Shell
**Bloquea:** T05, T06, T08, T13
**Bloqueada por:** T01, T02

## Descripcion

Definir la estructura central de estado de la app que mantiene todo el contexto.

## Modelo de datos

```rust
pub enum AppMode {
    Normal,     // Viendo PTY output
    Diff,       // Viendo diff side-by-side
}

pub enum InputFocus {
    WorkspaceList,  // Panel izquierdo superior
    FileList,       // Panel izquierdo inferior
    Terminal,       // Panel derecho (PTY input)
}

pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub focus: InputFocus,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,       // indice en workspaces
    pub selected_workspace: usize,     // cursor en la lista
    pub selected_file: usize,          // cursor en lista de archivos
    pub diff_scroll: u16,              // scroll vertical del diff
    pub diff_content: Option<Text<'static>>,  // diff renderizado
}

pub struct Workspace {
    pub name: String,
    pub branch: String,
    pub path: PathBuf,              // path del worktree
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    pub pty_parser: Arc<Mutex<vt100::Parser>>,
}

pub enum WorkspaceStatus {
    Idle,
    Busy,
    Done,
    Error(String),
}

pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,  // Modified, Added, Deleted, Renamed
}

pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}
```

## Acceptance Criteria

- [ ] Structs definidos en `app.rs`
- [ ] Metodos basicos: `App::new()`, `App::next_workspace()`, `App::prev_workspace()`
- [ ] Metodos de navegacion de archivos: `next_file()`, `prev_file()`
- [ ] `cargo check` compila
