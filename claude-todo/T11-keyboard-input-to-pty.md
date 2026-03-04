# T11 — Forwarding de keyboard input al PTY

**Status:** DONE
**Fase:** 3 — PTY Integration
**Bloquea:** T18
**Bloqueada por:** T03, T09

## Descripcion

Cuando el foco esta en el terminal (InputFocus::Terminal), reenviar los key events
al PTY de Claude Code. Debe manejar teclas especiales (Enter, Backspace, flechas, Ctrl+C, etc.)

## Detalle tecnico

```rust
// Mapeo de crossterm::event::KeyEvent a bytes para el PTY

fn key_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char(c)) => Some(c.to_string().into_bytes()),
        (KeyModifiers::SHIFT, KeyCode::Char(c)) => Some(c.to_uppercase().to_string().into_bytes()),
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(vec![3]),   // ETX
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => Some(vec![4]),   // EOT
        (KeyModifiers::CONTROL, KeyCode::Char('z')) => Some(vec![26]),  // SUB
        (_, KeyCode::Enter) => Some(vec![13]),     // CR
        (_, KeyCode::Backspace) => Some(vec![127]), // DEL
        (_, KeyCode::Tab) => Some(vec![9]),
        (_, KeyCode::Esc) => Some(vec![27]),
        (_, KeyCode::Up) => Some(b"\x1b[A".to_vec()),
        (_, KeyCode::Down) => Some(b"\x1b[B".to_vec()),
        (_, KeyCode::Right) => Some(b"\x1b[C".to_vec()),
        (_, KeyCode::Left) => Some(b"\x1b[D".to_vec()),
        _ => None,
    }
}
```

### Logica de focus

- **Ctrl+\\** (backslash): Toggle entre focus Terminal y focus WorkspaceList
  - Esto es el "escape hatch" para salir del modo terminal
- Cuando focus == Terminal: todos los key events van al PTY
- Cuando focus != Terminal: key events son manejados por la app (navegacion)

## Acceptance Criteria

- [x] Caracteres ASCII y UTF-8 se envian al PTY correctamente
- [x] Enter, Backspace, Tab, Esc funcionan
- [x] Flechas, Home, End, PageUp/Down, Delete, Insert envian secuencias ANSI
- [x] Ctrl+letra genérico (Ctrl+A..Z = bytes 1..26)
- [x] Ctrl+\\ toggle entre terminal y navegacion
- [x] F1-F12 envian secuencias correctas
