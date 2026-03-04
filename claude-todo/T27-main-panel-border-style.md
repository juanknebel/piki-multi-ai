# T27 — Border style del main panel segun foco

**Status:** DONE
**Fase:** 8 — Multi-repo support
**Bloquea:** —
**Bloqueada por:** —

## Descripcion

El main panel (terminal PTY y diff view) no refleja el estilo de borde
segun el foco activo. Los paneles izquierdos (workspace list, file list) si
usan `pane_border_style` (amarillo=selected, verde=interacting, gris=inactivo),
pero `ui/terminal.rs` hardcodea `Color::White` y `ui/diff.rs` hardcodea
`Color::Magenta`.

## Solucion

Pasar el `border_style` calculado desde `render_main_content` en `layout.rs`
a los renders de `terminal.rs` y `diff.rs`, usando `pane_border_style(app, ActivePane::MainPanel)`.

## Archivos a modificar

- `src/ui/layout.rs` — calcular border style y pasarlo
- `src/ui/terminal.rs` — recibir `Style` param para el borde
- `src/ui/diff.rs` — recibir `Style` param para el borde

## Acceptance Criteria

- [x] Al navegar con `l` al main panel, el borde se pone amarillo
- [x] Al presionar Enter (interact), el borde se pone verde
- [x] Al presionar Esc (back to nav), vuelve a amarillo
- [x] Al navegar fuera del main panel, el borde vuelve a gris
- [x] Diff view tambien respeta el color de borde
