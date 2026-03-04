# T17 — Scroll para la vista diff

**Status:** DONE
**Fase:** 5 — Diff View
**Bloquea:** T18
**Bloqueada por:** T16

## Descripcion

Implementar scroll vertical en la vista diff para archivos con diffs largos.

## Detalle tecnico

### Keybindings (cuando mode == Diff)
- `↑`/`k`: scroll up 1 linea
- `↓`/`j`: scroll down 1 linea
- `Page Up`/`Ctrl+u`: scroll up media pagina
- `Page Down`/`Ctrl+d`: scroll down media pagina
- `g`: ir al inicio
- `G`: ir al final
- `n`: siguiente archivo cambiado
- `p`: archivo anterior

### Limites de scroll

```rust
// Calcular max scroll
let total_lines = diff_content.lines.len() as u16;
let visible_lines = area.height.saturating_sub(2); // menos bordes
let max_scroll = total_lines.saturating_sub(visible_lines);

// Clamp
app.diff_scroll = app.diff_scroll.min(max_scroll);
```

### Indicador de posicion

Mostrar en la status bar: `Line X/Y` o un porcentaje.

## Acceptance Criteria

- [x] Scroll vertical funciona con flechas y j/k
- [x] Page up/down con Ctrl+u/d
- [x] g/G para inicio/final
- [x] n/p para navegar entre archivos
- [x] Scroll no va mas alla del contenido
- [x] Indicador de posicion visible
