# T05 — Layout basico con paneles y bordes

**Status:** DONE
**Fase:** 1 — Core App Shell
**Bloquea:** T08, T13, T16, T19
**Bloqueada por:** T03, T04

## Descripcion

Renderizar el layout principal de la TUI con paneles vacios pero con bordes y titulos.
Esto establece la estructura visual antes de llenarla con widgets funcionales.

## Layout

```
Horizontal split: [20%, 80%]

Panel izquierdo → Vertical split: [50%, 50%]
  - Superior: Block "WORKSPACES"
  - Inferior: Block "CHANGED FILES"

Panel derecho → Vertical split: [auto, 3, 1]
  - Tabs bar (3 filas)
  - Contenido principal (PTY o Diff) (fill)
  - Status bar (1 fila)

Footer global: 1 fila con keybindings
```

## Detalle tecnico

Usar `Layout::horizontal` y `Layout::vertical` con `Constraint`:
- Panel izq: `Constraint::Percentage(20)`
- Panel der: `Constraint::Percentage(80)`
- Tabs: `Constraint::Length(3)`
- Status: `Constraint::Length(1)`
- Footer: `Constraint::Length(1)`

## Acceptance Criteria

- [ ] Layout visible con bordes y titulos placeholder
- [ ] Responsivo al resize de terminal
- [ ] Funcion `ui::layout::render(frame, &app)` implementada
- [ ] Cada panel renderiza un Block con titulo
