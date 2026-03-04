# T08 — Widget lista de workspaces

**Status:** DONE
**Fase:** 2 — Workspace Management
**Bloquea:** T19
**Bloqueada por:** T05, T06, T07

## Descripcion

Renderizar la lista de workspaces en el panel izquierdo superior con seleccion
y highlight del workspace activo.

## Detalle visual

```
┌ WORKSPACES ──────┐
│                   │
│  ▶ ws-1 (active)  │  ← seleccionado + activo: bold + highlight
│    feature/login  │  ← nombre del branch
│    ● idle         │  ← estado de claude
│    3 files        │  ← cantidad de archivos cambiados
│                   │
│    ws-2           │  ← no seleccionado
│    fix/bug-123    │
│    ○ busy         │
│    1 file         │
│                   │
└───────────────────┘
```

## Detalle tecnico

Usar `ratatui::widgets::List` con items custom que contengan multiples lineas.
El item seleccionado se resalta con `Style::default().bg(Color::DarkGray)`.
El workspace activo tiene un `▶` como prefijo.

### Navegacion
- `↑`/`↓` o `j`/`k`: mover cursor en la lista
- `Enter` o `Tab`: activar el workspace seleccionado

## Acceptance Criteria

- [x] Lista renderizada con nombre, branch, status, file count
- [x] Highlight visual del item seleccionado
- [x] Indicador `▶` para el workspace activo
- [x] Navegacion con flechas y j/k funcional
- [x] Scroll si hay mas workspaces que filas disponibles
