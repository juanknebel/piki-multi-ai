# agent-multi — Plan General

## Vision

TUI en Rust (ratatui) para orquestar multiples instancias de Claude Code en paralelo,
cada una en su propio git worktree aislado. Inspirado en superset.sh pero como POC minimalista.

## Dependencias Externas (ya instaladas)

| Herramienta | Version  | Proposito                              |
|-------------|----------|----------------------------------------|
| delta       | 0.18.2   | Pager de diffs side-by-side con ANSI   |
| claude      | 2.1.63   | Claude Code CLI                        |
| git         | 2.53.0   | Worktrees, diffs, tracking             |
| rustc       | 1.92.0   | Compilador Rust                        |
| cargo       | 1.92.0   | Build system                           |

## Crates Rust

| Crate        | Version | Proposito                                       |
|--------------|---------|--------------------------------------------------|
| ratatui      | 0.30    | Framework TUI (immediate-mode, crossterm backend)|
| crossterm    | (re-export de ratatui)  | Backend de terminal              |
| tokio        | 1.x     | Async runtime para procesos hijos y channels     |
| portable-pty | 0.9     | Spawn Claude Code en PTY (sync, wrap con tokio)  |
| vt100        | 0.16    | Parser de ANSI/VT100 para estado de terminal     |
| tui-term     | 0.3     | Widget ratatui que renderiza vt100::Screen        |
| ansi-to-tui  | 8.0     | Convierte ANSI output de delta a ratatui::Text   |
| notify       | 8.2     | File watcher para detectar cambios en worktrees  |
| serde        | 1.x     | Serialization                                    |
| serde_json   | 1.x     | Config JSON                                      |
| anyhow       | 1.x     | Error handling                                   |

## Layout de la TUI

```
┌──────────────────┬──────────────────────────────────────────────────────────┐
│  WORKSPACES      │  ┌─── ws-1 ───┬─── ws-2 ───┬─── ws-3 ───┐            │
│  (20%)           │  │ TABS                                    │  (80%)    │
│  ▶ ws-1 (active) │  ├────────────────────────────────────────────────────┤
│    ws-2          │  │                                                    │
│    ws-3          │  │  MODO PTY: tui-term (Claude Code vivo)             │
│                  │  │  MODO DIFF: ansi-to-tui (delta side-by-side)       │
├──────────────────┤  │                                                    │
│  CHANGED FILES   │  │                                                    │
│  (ws activo)     │  │                                                    │
│                  │  │                                                    │
│  M src/auth.rs   │  ├────────────────────────────────────────────────────┤
│  A src/new.rs    │  │  STATUS BAR                                        │
│  M Cargo.toml    │  └────────────────────────────────────────────────────┘
├──────────────────┴──────────────────────────────────────────────────────────┤
│  [n]ew  [d]elete  [Tab]switch  [Enter]diff  [Esc]back  [q]uit             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Vista Diff Side-by-Side

Cuando se presiona Enter sobre un archivo, el panel derecho cambia a modo diff:

```
┌──────────────────────────────────────────────────┐
│  BEFORE (HEAD)          │  AFTER (working tree)  │
│  src/auth.rs            │  src/auth.rs           │
├─────────────────────────┼────────────────────────┤
│  fn authenticate(       │  fn authenticate(      │
│      user: &str         │      user: &str        │
│  ) -> Result<Token> {   │  ) -> Result<Token> {  │
│ -    todo!()            │ +    let creds =        │
│                         │ +        validate()?;   │
│                         │ +    Ok(Token::new())   │
│  }                      │  }                     │
└─────────────────────────┴────────────────────────┘
```

Se logra con:
```bash
git diff --color=always HEAD -- <file> | delta --side-by-side --width <W> --paging never --true-color always --line-fill-method ansi
```

## Decisiones Arquitecturales

1. **portable-pty es sync** → Wrap con `tokio::task::spawn_blocking` para leer el PTY
2. **vt100 + tui-term** para renderizar el terminal de Claude Code en vivo
3. **ansi-to-tui** para renderizar el output de delta (diffs estaticos)
4. **notify** con bridge a tokio channels para file watching async
5. **Un tokio task por workspace** que lee el PTY en background
6. **Estado centralizado** en `App` struct, accedido desde el main loop

## Fases de Implementacion

- **Fase 0**: Setup del proyecto (Cargo.toml, estructura de directorios)
- **Fase 1**: Shell de la app (main loop, estado, layout basico)
- **Fase 2**: Workspace management (git worktrees CRUD, widget lista)
- **Fase 3**: PTY integration (spawn claude, renderizar con tui-term)
- **Fase 4**: File watching (notify, lista de archivos cambiados)
- **Fase 5**: Diff view (delta side-by-side, ansi-to-tui, scroll)
- **Fase 6**: Tabs y multi-workspace (tab bar, switch, concurrencia)
- **Fase 7**: Polish (status bar, help, cleanup on exit)
