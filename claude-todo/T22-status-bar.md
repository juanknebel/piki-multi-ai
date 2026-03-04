# T22 — Status bar

**Status:** DONE
**Fase:** 7 — Polish
**Bloquea:** —
**Bloqueada por:** T05

## Descripcion

Barra de estado en la parte inferior del panel derecho (arriba del footer global).
Muestra informacion contextual del workspace activo.

## Detalle visual

### Modo PTY
```
 branch: feature/login │ 3 files changed │ claude: idle │ ws 1/3
```

### Modo Diff
```
 DIFF: src/auth.rs │ +7 -1 │ Line 12/45 │ [Esc] back │ [n/p] next/prev file
```

## Detalle tecnico

```rust
// ui/statusbar.rs

pub fn render_statusbar(
    frame: &mut Frame,
    area: Rect,
    app: &App,
) {
    let content = match app.mode {
        AppMode::Normal => {
            let ws = &app.workspaces[app.active_workspace];
            format!(
                " branch: {} │ {} files │ claude: {} │ ws {}/{}",
                ws.branch,
                ws.file_count(),
                ws.status_label(),
                app.active_workspace + 1,
                app.workspaces.len(),
            )
        }
        AppMode::Diff => {
            let file = &app.current_files()[app.selected_file];
            format!(
                " DIFF: {} │ [Esc] back │ [↑↓] scroll │ [n/p] file",
                file.path,
            )
        }
    };

    let bar = Paragraph::new(content)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(bar, area);
}
```

## Acceptance Criteria

- [x] Status bar visible en la parte inferior del panel derecho
- [x] Muestra info contextual segun el modo (PTY vs Diff)
- [x] Se actualiza en tiempo real
