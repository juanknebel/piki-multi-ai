# T33 — Sub-tabs de asistentes AI en el panel principal

**Status:** OPEN
**Phase:** 11 — Multi-Assistant Support
**Blocks:** —
**Blocked by:** —

## Description

Agregar un sistema de sub-pestañas dentro del panel principal (donde actualmente corre solo Claude Code) para soportar múltiples asistentes AI por workspace. Inicialmente: **Claude Code** (`claude`) y **Gemini** (`gemini`). El diseño debe ser extensible para agregar nuevos asistentes con cambios mínimos.

## Current Behavior

- Cada `Workspace` tiene un único `pty_session: Option<PtySession>` y un único `pty_parser`
- El panel principal siempre renderiza el PTY de Claude Code con título hardcoded `" Claude Code "`
- El input de teclado se envía siempre al único PTY del workspace activo

## Expected Behavior

- Cada workspace tiene **N sesiones PTY** (una por asistente configurado)
- El panel principal muestra sub-pestañas: `[Claude Code] [Gemini]`
- El usuario puede cambiar de sub-pestaña para ver/interactuar con cada asistente
- El input de teclado va al PTY del asistente activo
- Los asistentes corren en paralelo (ambos PTYs vivos simultáneamente)
- Agregar un nuevo asistente en el futuro requiere solo: añadir un variante al enum + su comando

## Design

### 1. Nuevo enum `AIProvider` (`app.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AIProvider {
    Claude,
    Gemini,
}

impl AIProvider {
    /// Comando CLI a ejecutar
    pub fn command(&self) -> &str {
        match self {
            AIProvider::Claude => "claude",
            AIProvider::Gemini => "gemini",
        }
    }

    /// Label para la sub-pestaña
    pub fn label(&self) -> &str {
        match self {
            AIProvider::Claude => "Claude Code",
            AIProvider::Gemini => "Gemini",
        }
    }

    /// Lista ordenada de todos los providers disponibles
    pub fn all() -> &'static [AIProvider] {
        &[AIProvider::Claude, AIProvider::Gemini]
    }
}
```

### 2. Cambios en `Workspace` (`app.rs`)

Reemplazar los campos únicos de PTY por colecciones indexadas por provider:

```rust
pub struct Workspace {
    // ... campos existentes sin cambio ...
    // REEMPLAZAR:
    //   pub pty_session: Option<PtySession>,
    //   pub pty_parser: Arc<Mutex<vt100::Parser>>,
    // POR:
    pub pty_sessions: HashMap<AIProvider, PtySession>,
    pub pty_parsers: HashMap<AIProvider, Arc<Mutex<vt100::Parser>>>,
    pub active_provider: AIProvider,
}
```

### 3. Cambios en `PtySession::spawn` (`pty/session.rs`)

Parametrizar el comando a ejecutar. Actualmente hardcodea `"claude"`:

```rust
// Antes:
pub async fn spawn(workdir: &Path, rows: u16, cols: u16) -> ...
// Después:
pub async fn spawn(workdir: &Path, rows: u16, cols: u16, command: &str) -> ...
```

### 4. Nuevo widget sub-tabs (`ui/subtabs.rs`)

Renderizar las sub-pestañas del asistente activo justo debajo de las workspace tabs:

```
┌─────────────────────────────────┐
│ [ws-1] [ws-2] [ws-3]           │  ← workspace tabs (existente)
│ [Claude Code] [Gemini]         │  ← sub-tabs (NUEVO)
│                                 │
│  ... PTY output ...             │  ← terminal del provider activo
│                                 │
└─────────────────────────────────┘
```

### 5. Cambios en layout (`ui/layout.rs`)

- Agregar `sub_tabs_area` (1 línea) entre `tabs_area` y `main_area`
- `render_main_content()` usa `ws.active_provider` para elegir qué parser renderizar
- `render()` en `ui/terminal.rs` recibe el label dinámico del provider

### 6. Routing de input (`main.rs`)

En `handle_terminal_interaction()`, enviar bytes al PTY del provider activo:

```rust
if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
    if let Some(pty) = ws.pty_sessions.get_mut(&ws.active_provider) {
        if let Some(bytes) = pty::input::key_to_bytes(key) {
            let _ = pty.write(&bytes);
        }
    }
}
```

### 7. Navegación entre sub-tabs

- **En modo navegación (no interacting)**: tecla dedicada (ej: `g` para toggle, o `[`/`]` para prev/next provider)
- Alternativamente `Ctrl+1`, `Ctrl+2`, etc. para ir directo a un provider

### 8. Spawn de PTYs al crear workspace (`main.rs`)

Al crear o restaurar un workspace, spawnar un PTY por cada provider:

```rust
for provider in AIProvider::all() {
    match PtySession::spawn(&ws.path, rows, cols, provider.command()).await {
        Ok(session) => {
            ws.pty_parsers.insert(*provider, Arc::clone(session.parser()));
            ws.pty_sessions.insert(*provider, session);
        }
        Err(e) => { /* log error, no crash */ }
    }
}
ws.active_provider = AIProvider::Claude; // default
```

### 9. Cleanup al salir

Iterar todos los PTY sessions de cada provider al hacer kill:

```rust
for (_, pty) in ws.pty_sessions.iter_mut() {
    let _ = pty.kill();
}
```

## Files to modify

| File | Cambio |
|------|--------|
| `src/app.rs` | Nuevo enum `AIProvider`, cambiar campos de `Workspace` a `HashMap<AIProvider, ...>`, agregar `active_provider` |
| `src/pty/session.rs` | Parametrizar comando en `spawn()` (cambiar `"claude"` hardcoded por parámetro) |
| `src/ui/subtabs.rs` | **NUEVO** — Widget de sub-pestañas de providers |
| `src/ui/mod.rs` | Exportar nuevo módulo `subtabs` |
| `src/ui/layout.rs` | Agregar sub-tabs area en layout, cambiar `render_main_content()` para usar provider activo |
| `src/ui/terminal.rs` | Título dinámico basado en provider |
| `src/main.rs` | Spawn múltiples PTYs, routing input por provider, keybinding para cambiar sub-tab, cleanup |

## Extensibility

Para agregar un nuevo asistente (ej: `aider`, `copilot`):

1. Agregar variante a `AIProvider` enum
2. Implementar `command()` y `label()` para la nueva variante
3. Agregar a `AIProvider::all()`
4. Todo lo demás (spawn, tabs, routing, cleanup) funciona automáticamente

## Acceptance Criteria

- [ ] Cada workspace spawna PTYs para Claude y Gemini al crearse
- [ ] Sub-pestañas visibles debajo de las workspace tabs
- [ ] Se puede cambiar entre asistentes con keybinding
- [ ] El input va al PTY del asistente activo
- [ ] Los asistentes que no están en foco siguen corriendo en background
- [ ] Si un asistente no está instalado (`command not found`), mostrar error en status sin crash
- [ ] `cargo build` compila
- [ ] `cargo clippy` limpio
- [ ] `cargo test` pasa
