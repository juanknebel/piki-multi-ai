# T19 — Tab bar widget

**Status:** DONE
**Fase:** 6 — Tabs & Multi-workspace
**Bloquea:** T20
**Bloqueada por:** T03, T05, T08

## Descripcion

Renderizar una barra de tabs en la parte superior del panel derecho.
Cada tab representa un workspace. El tab activo esta resaltado.

## Detalle visual

```
┌─── ws-1 ───┬─── ws-2 ───┬─── ws-3 ───┬──────────────────┐
│  (active)   │             │             │                  │
```

## Detalle tecnico

Usar `ratatui::widgets::Tabs`:

```rust
// ui/tabs.rs

pub fn render_tabs(
    frame: &mut Frame,
    area: Rect,
    workspaces: &[Workspace],
    active: usize,
) {
    let titles: Vec<Line> = workspaces.iter()
        .enumerate()
        .map(|(i, ws)| {
            let style = if i == active {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(ws.name.clone()).style(style)
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(active)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .divider(" | ");

    frame.render_widget(tabs, area);
}
```

## Acceptance Criteria

- [x] Tabs visibles con nombres de workspaces
- [x] Tab activo resaltado con color distinto
- [x] Se actualiza cuando se crea/elimina un workspace
- [x] Se actualiza cuando se cambia de workspace activo
