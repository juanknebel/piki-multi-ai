# T18 — Toggle entre modo PTY y modo Diff

**Status:** DONE
**Fase:** 5 — Diff View
**Bloquea:** —
**Bloqueada por:** T10, T11, T13, T16, T17

## Descripcion

Implementar la logica de cambio entre los dos modos del panel derecho:
- **PTY mode**: muestra el terminal de Claude Code (tui-term)
- **Diff mode**: muestra el diff side-by-side (ansi-to-tui)

## Flujo

```
PTY Mode (default)
  │
  │  [focus en FileList] + Enter en archivo
  ▼
Diff Mode
  │
  │  Esc → vuelve a PTY mode
  │  n/p → cambia archivo, se queda en Diff mode
  │
  ▼
PTY Mode
```

## Detalle tecnico

```rust
// En el handler de eventos del main loop:

match app.mode {
    AppMode::Normal => {
        // Panel derecho renderiza tui-term
        // Input va al PTY (si focus == Terminal)
        // o navega workspaces/files (si focus != Terminal)

        if key == Enter && app.focus == InputFocus::FileList {
            // Obtener archivo seleccionado
            let file = &app.current_workspace().changed_files[app.selected_file];
            // Ejecutar diff async
            let diff_bytes = run_diff(&ws.path, &file.path, panel_width).await?;
            app.diff_content = Some(diff_bytes.into_text()?);
            app.diff_scroll = 0;
            app.mode = AppMode::Diff;
        }
    }
    AppMode::Diff => {
        // Panel derecho renderiza diff
        // Keybindings de scroll activos
        match key {
            Esc => {
                app.mode = AppMode::Normal;
                app.diff_content = None;
            }
            // scroll keys...
        }
    }
}
```

### Render dispatch

```rust
// ui/layout.rs
match app.mode {
    AppMode::Normal => ui::terminal::render_terminal(frame, right_area, &parser),
    AppMode::Diff => ui::diff::render_diff(frame, right_area, &app.diff_content, app.diff_scroll, &file),
}
```

## Acceptance Criteria

- [x] Enter en archivo cambia a modo Diff
- [x] Esc en modo Diff vuelve a modo PTY
- [x] El PTY sigue corriendo en background durante modo Diff
- [x] La transicion es instantanea (no se pierde estado del PTY)
- [x] n/p navega entre archivos sin volver a PTY mode
