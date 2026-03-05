# T51: Fuzzy Search y Navegacion de Archivos

**Status:** DONE
**Bloqueada por:** —
**Branch:** feature/fuzzy-search

## Descripcion

Agregar fuzzy search nativo al proyecto usando el crate `nucleo` (mismo motor que Helix editor) para buscar y navegar entre archivos del worktree activo. Incluye tres acciones sobre el archivo seleccionado: ver diff, abrir en $EDITOR externo, y edicion inline con `tui-textarea`.

## Keybindings

| Keybind        | Accion                                                      |
|----------------|-------------------------------------------------------------|
| `Ctrl+Shift+F` | Abre el fuzzy search overlay (AppMode::FuzzySearch)        |
| `Enter`        | (En fuzzy search) Selecciona archivo y abre su diff en MainPanel |
| `e`            | (En file list/fuzzy result) Suspende TUI, abre $EDITOR, retoma al cerrar |
| `v`            | (En file list/fuzzy result) Abre el archivo en modo edicion inline con tui-textarea |
| `Esc`          | Cierra el fuzzy search overlay / cierra el editor inline   |

## Dependencias (crates a agregar)

- `nucleo` — motor de fuzzy matching (puro Rust, async-friendly)
- `ignore` — walk de archivos respetando .gitignore
- `tui-textarea` — widget de edicion multilinea para ratatui (syntax highlight, undo/redo, search)

## Sub-tareas de Implementacion

### ST1: Nuevo AppMode::FuzzySearch

1. Agregar variante `FuzzySearch` a `AppMode` en `src/app.rs`
2. Agregar estado del fuzzy finder al `App` struct:
   - `fuzzy_query: String` — texto del input
   - `fuzzy_results: Vec<String>` — archivos filtrados
   - `fuzzy_selected: usize` — indice seleccionado en la lista
   - `fuzzy_all_files: Vec<String>` — cache de todos los archivos del worktree
3. Metodo `App::open_fuzzy_search()` que:
   - Usa `ignore::WalkBuilder` para escanear archivos del worktree activo (respeta .gitignore)
   - Inicializa el matcher de nucleo con la lista de archivos
   - Cambia mode a `AppMode::FuzzySearch`

### ST2: UI del Fuzzy Search Overlay

1. Crear `src/ui/fuzzy.rs` con funcion `render(frame, area, app)`
2. Renderizar como popup centrado (similar a NewWorkspace dialog):
   - Input de texto arriba (query)
   - Lista de resultados debajo con highlight del match
   - Indicador de cantidad de resultados (ej. "12/345")
3. Resaltar las letras matcheadas en cada resultado (nucleo provee los indices)
4. Integrar con el theme actual (`app.theme`)

### ST3: Key Handling del Fuzzy Search

1. En `handle_key_event` de `src/main.rs`, cuando `mode == FuzzySearch`:
   - Caracteres alfanumericos -> agregar a `fuzzy_query`, re-filtrar
   - Backspace -> borrar ultimo char, re-filtrar
   - Up/Down (o Ctrl+p/Ctrl+n) -> mover seleccion
   - Enter -> ejecutar accion sobre archivo seleccionado (ir a diff)
   - Esc -> cerrar fuzzy search, volver a modo anterior
2. Interceptar `Ctrl+Shift+F` en modo Normal/navigation para abrir el fuzzy search
3. El filtrado con nucleo debe ser incremental (no re-escanear archivos cada vez)

### ST4: Accion Enter — Abrir Diff del Archivo

1. Al presionar Enter en fuzzy search:
   - Obtener el path del archivo seleccionado
   - Si el archivo esta en `changed_files` del workspace activo, seleccionarlo en FileList
   - Cambiar a `AppMode::Diff` y renderizar el diff de ese archivo
   - Si el archivo NO tiene cambios, mostrar mensaje en statusbar ("No changes for this file")

### ST5: Accion `e` — Abrir en $EDITOR Externo

1. Implementar funcion `open_in_editor(file_path: &str)` (probablemente en `src/app.rs` o utility):
   ```rust
   // Pseudocodigo
   fn open_in_editor(terminal: &mut DefaultTerminal, file_path: &Path) -> Result<()> {
       // Deshabilitar mouse capture
       crossterm::execute!(stderr(), DisableMouseCapture)?;
       // Restaurar terminal
       ratatui::restore();
       // Lanzar editor
       let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
       std::process::Command::new(&editor)
           .arg(file_path)
           .status()?;
       // Re-inicializar terminal
       *terminal = ratatui::init();
       crossterm::execute!(stderr(), EnableMouseCapture)?;
       Ok(())
   }
   ```
2. Disponible tanto desde fuzzy search como desde file list (pane FileList)
3. Despues de cerrar el editor, marcar workspace como `dirty` para refrescar file status

### ST6: Accion `v` — Editor Inline con tui-textarea

1. Agregar variante `AppMode::InlineEdit` a `AppMode`
2. Agregar al `App` struct:
   - `textarea: Option<TextArea<'static>>` — instancia del widget tui-textarea
   - `editing_file: Option<PathBuf>` — path del archivo siendo editado
3. Al presionar `v`:
   - Leer contenido del archivo
   - Crear `TextArea::new(lines)` con syntax highlighting si es posible
   - Cambiar a `AppMode::InlineEdit`
4. Key handling en modo InlineEdit:
   - Todas las teclas van al textarea (ya maneja undo, search, etc.)
   - `Ctrl+S` -> guardar archivo a disco
   - `Esc` -> cerrar sin guardar (o preguntar si hay cambios)
5. Renderizar el textarea en el MainPanel area
6. Al cerrar, marcar workspace como `dirty`

### ST7: Integracion con UI existente

1. Registrar `Ctrl+Shift+F` en el help overlay (`src/ui/layout.rs` o help data)
2. Registrar `e` y `v` en el help overlay
3. Actualizar statusbar para mostrar hints contextuales:
   - En FuzzySearch: "Enter: open diff | e: $EDITOR | v: inline edit | Esc: close"
   - En InlineEdit: "Ctrl+S: save | Esc: close"

## Archivos a Modificar

- `Cargo.toml` — agregar nucleo, ignore, tui-textarea
- `src/app.rs` — nuevos AppMode, campos en App struct
- `src/main.rs` — key handling para nuevos modos y keybinds
- `src/ui/mod.rs` — registrar nuevo modulo fuzzy
- `src/ui/fuzzy.rs` — **NUEVO** — render del overlay fuzzy search
- `src/ui/layout.rs` — integrar render del fuzzy overlay y inline editor
- `src/ui/statusbar.rs` — hints para nuevos modos
- `src/ui/layout.rs` (help section) — documentar nuevos keybinds

## Notas Tecnicas

- `nucleo` soporta matching incremental: al agregar/borrar caracteres no recalcula todo, solo actualiza. Usar `nucleo::Nucleo` (la version threaded) o `nucleo::pattern::Pattern` (single-thread, mas simple para empezar).
- `ignore::WalkBuilder` es del mismo autor que ripgrep, respeta .gitignore, .ignore, y hidden files por defecto.
- `tui-textarea` se integra directamente como widget de ratatui: `frame.render_widget(textarea.widget(), area)`.
- Para `Ctrl+Shift+F`: crossterm reporta `KeyCode::Char('F')` (mayuscula) con `KeyModifiers::CONTROL | KeyModifiers::SHIFT`. Verificar que no colisione con otros bindings.
- La suspension de TUI para $EDITOR debe restaurar correctamente mouse capture y raw mode.

## Criterios de Aceptacion

- [ ] Ctrl+Shift+F abre popup de fuzzy search sobre el worktree activo
- [ ] Escribir filtra archivos en tiempo real con highlighting de matches
- [ ] Enter sobre un archivo abre su diff en MainPanel
- [ ] `e` sobre un archivo suspende TUI y abre $EDITOR, al cerrar retoma la app
- [ ] `v` sobre un archivo abre editor inline con tui-textarea en MainPanel
- [ ] Ctrl+S en editor inline guarda el archivo
- [ ] Esc cierra fuzzy search o editor inline
- [ ] Help overlay documenta los nuevos keybinds
- [ ] Compila sin warnings, pasa `cargo clippy`, pasa `cargo test`
