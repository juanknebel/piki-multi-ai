# T23 — Help overlay

**Status:** DONE
**Fase:** 7 — Polish
**Bloquea:** —
**Bloqueada por:** T05

## Descripcion

Popup de ayuda que se muestra al presionar `?`. Lista todos los keybindings disponibles.

## Detalle visual

```
┌─────────── Help ─────────────────┐
│                                   │
│  Global                           │
│    q          Quit                │
│    ?          Toggle help         │
│    n          New workspace       │
│    d          Delete workspace    │
│    Tab        Next workspace      │
│    Shift+Tab  Prev workspace      │
│    1-9        Go to workspace N   │
│    Ctrl+\     Toggle terminal     │
│                                   │
│  File List                        │
│    j/↓        Next file           │
│    k/↑        Prev file           │
│    Enter      View diff           │
│                                   │
│  Diff View                        │
│    Esc        Back to terminal    │
│    j/↓        Scroll down         │
│    k/↑        Scroll up           │
│    Ctrl+d     Page down           │
│    Ctrl+u     Page up             │
│    g          Go to top           │
│    G          Go to bottom        │
│    n          Next file           │
│    p          Prev file           │
│                                   │
│          [?] Close                │
└───────────────────────────────────┘
```

## Detalle tecnico

Usar `ratatui::widgets::Clear` + `Block` centrado sobre el contenido existente.

```rust
// Calcular area centrada
let popup_area = centered_rect(60, 80, frame.area());
frame.render_widget(Clear, popup_area);
frame.render_widget(help_widget, popup_area);
```

## Acceptance Criteria

- [x] `?` muestra/oculta el popup de help
- [x] Popup centrado y con borde
- [x] Lista todos los keybindings
- [x] Cualquier tecla cierra el popup
