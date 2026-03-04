# T16 — Renderizado del diff con ansi-to-tui

**Status:** DONE
**Fase:** 5 — Diff View
**Bloquea:** T17, T18
**Bloqueada por:** T05, T15

## Descripcion

Convertir el output ANSI de delta a `ratatui::text::Text` usando `ansi-to-tui`
y renderizarlo en el panel derecho como un Paragraph con scroll.

## Detalle tecnico

```rust
// ui/diff.rs

use ansi_to_tui::IntoText;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub fn render_diff(
    frame: &mut Frame,
    area: Rect,
    diff_content: &Option<Text<'static>>,
    scroll: u16,
    file_path: &str,
) {
    if let Some(text) = diff_content {
        let paragraph = Paragraph::new(text.clone())
            .block(Block::default()
                .title(format!(" DIFF: {} ", file_path))
                .borders(Borders::ALL))
            .scroll((scroll, 0));
        frame.render_widget(paragraph, area);
    } else {
        // Placeholder cuando no hay diff seleccionado
        let paragraph = Paragraph::new("Select a file to view diff")
            .block(Block::default()
                .title(" DIFF ")
                .borders(Borders::ALL));
        frame.render_widget(paragraph, area);
    }
}
```

### Conversion ANSI → Text

```rust
// Cuando el usuario selecciona un archivo:
let ansi_bytes = diff::runner::run_diff(&worktree_path, &file_path, width).await?;
let text: Text = ansi_bytes.into_text()?;
app.diff_content = Some(text);
app.diff_scroll = 0;
app.mode = AppMode::Diff;
```

### Nota sobre side-by-side

Delta ya formatea el output en dos columnas. El ancho pasado a delta debe ser
el ancho del area del widget (menos bordes = area.width - 2).
`ansi-to-tui` preserva el layout de columnas de delta porque mantiene los espacios.

## Acceptance Criteria

- [x] Diff renderizado con colores de delta preservados
- [x] Side-by-side visible: columna izq (before) y columna der (after)
- [x] Titulo muestra nombre del archivo
- [x] Placeholder cuando no hay diff seleccionado
- [x] Colores: rojo para eliminado, verde para agregado
