# T13 — Widget lista de archivos cambiados

**Status:** DONE
**Fase:** 4 — File Watching
**Bloquea:** T15, T18
**Bloqueada por:** T04, T05, T12, T14

## Descripcion

Renderizar la lista de archivos cambiados del workspace activo en el panel
izquierdo inferior. Cada archivo muestra su status (M/A/D/R) con color.

## Detalle visual

```
┌ CHANGED FILES ────┐
│  (ws-1)           │
│                   │
│  M src/auth.rs    │  ← amarillo (Modified)
│  A src/session.rs │  ← verde (Added)
│  D old_file.rs    │  ← rojo (Deleted)
│  M Cargo.toml     │  ← amarillo
│                   │
│  ◀ seleccionado   │  ← highlight + indicador
└───────────────────┘
```

## Detalle tecnico

Usar `ratatui::widgets::List` con items coloreados segun status:
- `M` → Yellow
- `A` → Green
- `D` → Red
- `R` → Cyan

El item seleccionado se resalta con `Style::default().bg(Color::DarkGray)`.

### Navegacion (cuando focus == FileList)
- `↑`/`↓` o `j`/`k`: mover cursor
- `Enter`: abrir diff del archivo seleccionado
- `Esc` o `Tab`: volver focus a WorkspaceList

## Acceptance Criteria

- [x] Lista muestra archivos con status y colores
- [x] Se actualiza cuando el file watcher detecta cambios
- [x] Se actualiza cuando se cambia de workspace activo
- [x] Navegacion con flechas y j/k
- [x] Enter abre diff del archivo seleccionado
- [x] Scroll si hay mas archivos que filas
