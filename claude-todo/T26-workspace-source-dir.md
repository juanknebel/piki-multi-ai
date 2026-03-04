# T26 — Directorio por workspace + comando claude

**Status:** DONE
**Fase:** 8 — Multi-repo support
**Bloquea:** —
**Bloqueada por:** T25

## Descripcion

Cambiar agent-multi para que cada workspace apunte a un repo git distinto
(ingresado por el usuario en el dialogo), y que el comando sea simplemente
`claude` (sin `--dangerously-skip-permissions`).

## Sub-tareas

### T26a — app.rs: DialogField, source_repo, dir_input_buffer

- Agregar enum `DialogField { Name, Directory }`
- Agregar `source_repo: PathBuf` a `Workspace`
- `Workspace::new()` recibe `source_repo: PathBuf` adicional
- Agregar `dir_input_buffer: String` y `active_dialog_field: DialogField` a `App`

### T26b — main.rs: Action, execute_action, dialog input

- `Action::CreateWorkspace(String)` → `Action::CreateWorkspace(String, PathBuf)`
- `run()` ya no requiere estar en un git repo — inicia con vec vacio
- `execute_action` pasa `dir` a `manager.create()` y `source_repo` a `manager.remove()`
- `handle_new_workspace_input` reescrita:
  - Tab alterna entre campos Name y Directory
  - Enter valida ambos campos + resuelve `~` + verifica que el directorio exista
  - Filtrado por campo: Name solo alfanumerico/-/_, Directory cualquier printable
- Al presionar `n` se resetean ambos buffers y el campo activo

### T26c — workspace/manager.rs: per-workspace source_dir

- `WorkspaceManager` ahora es stateless (sin `base_repo`/`worktrees_dir`)
- `new()` sin argumentos, no requiere git repo
- `create(name, source_dir)` detecta git root desde `source_dir`, crea worktree ahi
- `remove(name, source_repo)` usa el `source_repo` del workspace
- Eliminado `list()` (ya no hay un repo unico donde listar)

### T26d — pty/session.rs: Quitar --dangerously-skip-permissions

- Comando ahora es solo `claude` sin flags extra

### T26e — ui/layout.rs: Dialogo con 2 campos + Tab

- Popup 50x7 con dos campos (Name + Dir)
- Campo activo en amarillo con cursor `█`, inactivo en gris
- Footer actualizado con `[Tab] switch field`

### T26f — Cargo.toml: dependencia dirs

- Agregar `dirs = "6"` para resolver `~` a home dir

## Detalle visual

```
┌─── New Workspace ──────────────────────────┐
│                                             │
│  Name: my-feature█                          │
│                                             │
│  Dir:  ~/git/my-project                     │
│                                             │
└─────────────────────────────────────────────┘
Footer: [Tab] switch field  [Enter] create  [Esc] cancel
```

## Archivos modificados

- `src/app.rs` — DialogField, source_repo, dir_input_buffer
- `src/main.rs` — Action, execute_action, handle_new_workspace_input, run()
- `src/workspace/manager.rs` — WorkspaceManager stateless
- `src/pty/session.rs` — Quitar flag
- `src/ui/layout.rs` — Dialogo 2 campos
- `Cargo.toml` — dirs dependency

## Acceptance Criteria

- [x] `cargo build` compila sin errores
- [x] `cargo test` pasa (4 tests)
- [x] `n` abre dialogo con 2 campos (Name y Dir)
- [x] Tab alterna entre campos
- [x] Nombre + directorio valido → crea workspace con worktree en ese repo
- [x] Directorio inexistente → muestra error en status bar
- [x] Directorio que no es git repo → muestra error en status bar
- [x] PTY ejecuta `claude` (sin flags extra)
- [x] `d` elimina workspace y limpia worktree del repo fuente
- [x] Se puede ejecutar agent-multi desde cualquier directorio (no requiere git repo)
