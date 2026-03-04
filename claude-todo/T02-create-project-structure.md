# T02 — Crear estructura de directorios del proyecto

**Status:** DONE
**Fase:** 0 — Setup
**Bloquea:** T03, T04, T05
**Bloqueada por:** T01

## Descripcion

Crear la estructura de modulos y archivos vacios (con `mod` declarations).

## Estructura objetivo

```
src/
├── main.rs           # Entry point, setup tokio + terminal
├── app.rs            # Estado global de la app (App struct)
├── ui/
│   ├── mod.rs        # Re-exports
│   ├── layout.rs     # Layout principal (paneles izq/der)
│   ├── workspaces.rs # Widget lista de workspaces
│   ├── files.rs      # Widget lista de archivos cambiados
│   ├── terminal.rs   # Widget PTY (tui-term)
│   ├── diff.rs       # Widget diff side-by-side (ansi-to-tui)
│   ├── tabs.rs       # Widget tabs de workspaces
│   └── statusbar.rs  # Widget barra de estado
├── workspace/
│   ├── mod.rs        # Re-exports
│   ├── manager.rs    # CRUD de worktrees (git commands)
│   └── watcher.rs    # notify file watcher
├── pty/
│   ├── mod.rs        # Re-exports
│   └── session.rs    # Spawn claude en PTY, lectura async
└── diff/
    ├── mod.rs        # Re-exports
    └── runner.rs     # Ejecuta git diff | delta, captura ANSI
```

## Acceptance Criteria

- [ ] Todos los archivos creados con declaraciones `mod` correctas
- [ ] `cargo check` compila (archivos pueden tener contenido minimo/placeholder)
