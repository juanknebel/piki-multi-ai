# T10 — Renderizado del PTY con tui-term

**Status:** DONE
**Fase:** 3 — PTY Integration
**Bloquea:** T18
**Bloqueada por:** T09, T05

## Descripcion

Renderizar el output del PTY de Claude Code en el panel derecho usando tui-term.
El widget `PseudoTerminal` toma un `vt100::Screen` y lo convierte en celdas ratatui.

## Detalle tecnico

```rust
// ui/terminal.rs

use tui_term::widget::PseudoTerminal;
use ratatui::widgets::Block;

pub fn render_terminal(
    frame: &mut Frame,
    area: Rect,
    parser: &Arc<Mutex<vt100::Parser>>,
) {
    let parser = parser.lock().unwrap();
    let pseudo_term = PseudoTerminal::new(parser.screen())
        .block(Block::default()
            .title(" Claude Code ")
            .borders(Borders::ALL));
    frame.render_widget(pseudo_term, area);
}
```

### Consideraciones

- El parser se lockea brevemente para leer el screen durante el render
- El render ocurre cada ~50ms (tick rate del main loop)
- El tamano del PTY debe coincidir con el area del widget (menos bordes)
- Cuando el area cambia de tamano, se debe llamar `pty_session.resize()`

## Acceptance Criteria

- [x] Output de Claude Code visible en el panel derecho con colores (PseudoTerminal widget)
- [x] Cursor visible en la posicion correcta (vt100 screen)
- [x] Scroll del terminal funciona (vt100 scrollback)
- [x] Resize se maneja correctamente (PtySession::resize available)
