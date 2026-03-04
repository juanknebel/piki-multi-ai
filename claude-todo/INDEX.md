# agent-multi — Task Index

## Resumen

| Total | OPEN | IN_PROGRESS | DONE | CANCEL |
|-------|------|-------------|------|--------|
| 45    | 0    | 0           | 45   | 0      |

## Fase 0 — Setup

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T01 | Inicializar proyecto Cargo         | DONE   | —             |
| T02 | Crear estructura de directorios    | DONE   | T01           |

## Fase 1 — Core App Shell

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T03 | Main loop con event handling       | DONE   | T01, T02      |
| T04 | App state struct                   | DONE   | T01, T02      |
| T05 | Layout basico con paneles          | DONE   | T03, T04      |

## Fase 2 — Workspace Management

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T06 | Workspace model y estado           | DONE   | T04           |
| T07 | Git worktree CRUD                  | DONE   | T06           |
| T08 | Widget lista de workspaces         | DONE   | T05, T06, T07 |

## Fase 3 — PTY Integration

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T09 | Spawn Claude Code en PTY           | DONE   | T07           |
| T10 | Renderizado PTY con tui-term       | DONE   | T09, T05      |
| T11 | Keyboard input forwarding al PTY   | DONE   | T03, T09      |

## Fase 4 — File Watching

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T12 | File watcher con notify            | DONE   | T07           |
| T13 | Widget lista de archivos cambiados | DONE   | T04, T05, T12, T14 |
| T14 | Tracking via git diff --name-status| DONE   | T06, T12      |

## Fase 5 — Diff View

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T15 | Ejecutar git diff | delta s-b-s    | DONE   | T13           |
| T16 | Renderizado diff con ansi-to-tui   | DONE   | T05, T15      |
| T17 | Scroll para vista diff             | DONE   | T16           |
| T18 | Toggle modo PTY ↔ Diff             | DONE   | T10, T11, T13, T16, T17 |

## Fase 6 — Tabs & Multi-workspace

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T19 | Tab bar widget                     | DONE   | T03, T05, T08 |
| T20 | Switch entre workspaces            | DONE   | T19           |
| T21 | Sesiones PTY concurrentes          | DONE   | T09, T20      |

## Fase 7 — Polish

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T22 | Status bar                         | DONE   | T05           |
| T23 | Help overlay                       | DONE   | T05           |
| T24 | Cleanup al salir                   | DONE   | T07, T09      |
| T25 | Dialogo nuevo workspace            | DONE   | T07, T08      |

## Fase 8 — Multi-repo Support

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T26 | Directorio por workspace + claude  | DONE   | T25           |
| T27 | Border style main panel segun foco | DONE   | —             |

## Fase 9 — Mejoras

| ID  | Tarea                              | Status      | Bloqueada por |
|-----|------------------------------------|-------------|---------------|
| T28 | git status --porcelain en files    | DONE        | T14           |
| T29 | Renombrar panel a "STATUS"         | DONE        | —             |
| T30 | Worktrees en ~/.local/share/piki-multi | DONE    | —             |

## Phase 10 — Persistence

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T31 | Persist workspace config to disk   | DONE   | T30           |
| T32 | Fix config not loading on startup  | DONE   | T31           |

## Phase 11 — Multi-Assistant Support

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T33 | Sub-tabs asistentes AI en panel    | DONE        | —             |
| T34 | Add Codex provider                 | DONE        | T33           |

## Phase 12 — Naming

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T35 | Branch name matches workspace name | DONE   | —             |

## Phase 13 — UX Enhancements

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T36 | Add description field to workspaces| DONE   | —             |
| T37 | Checkout remote branch en worktree | DONE   | —             |

## Phase 14 — Bug Fixes

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T38 | Diff view hides provider shell pane| DONE   | —             |
| T39 | WS list: desc+path, branch chars   | DONE   | —             |
| T40 | Bug: WS pane foco oculta detalles  | DONE   | —             |
| T41 | Bug: popup nuevo WS muy pequeño    | DONE   | —             |

## Phase 15 — UX Improvements

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T42 | Confirmar borrado, mantener worktree| DONE  | —             |

## Phase 16 — Terminal Panel Improvements

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T43 | Dynamic PTY width + scrollback     | DONE   | —             |
| T44 | Shell tab in provider sub-tabs     | DONE   | —             |

## Phase 17 — Bug Fixes

| ID  | Tarea                              | Status | Bloqueada por |
|-----|------------------------------------|--------|---------------|
| T45 | Diff popup doesn't get focus       | DONE   | —             |

## Grafo de Dependencias

```
T01 ──→ T02 ──→ T03 ──→ T05 ──→ T08 ──→ T19 ──→ T20 ──→ T21
  │       │       │       │       │                 │
  │       │       │       ├──→ T10 ──→ T18          │
  │       │       │       ├──→ T16 ──→ T17 ──→ T18  │
  │       │       │       ├──→ T22                   │
  │       │       │       └──→ T23                   │
  │       │       │                                  │
  │       └──→ T04 ──→ T05                           │
  │              │                                    │
  │              ├──→ T06 ──→ T07 ──→ T08             │
  │              │             │                      │
  │              │             ├──→ T09 ──→ T10       │
  │              │             │     │                │
  │              │             │     └──→ T11 ──→ T18│
  │              │             │     └──→ T21 ←──────┘
  │              │             │     └──→ T24
  │              │             │
  │              │             ├──→ T12 ──→ T14 ──→ T13 ──→ T15
  │              │             │                      │
  │              │             ├──→ T25                │
  │              │             └──→ T24                │
  │              │                                    │
  │              └──→ T13                             │
  │                                                   │
  └──→ T03 ──→ T11                                   │
                                                      │
                                    T15 ──→ T16 ──→ T17 ──→ T18
```

## Camino Critico

El path mas largo (determinante del tiempo total):

```
T01 → T02 → T04 → T06 → T07 → T09 → T10 → T18
                                       ↓
T01 → T02 → T03 → T05 → T16 → T17 → T18
```

## Tareas Paralelizables

Estas tareas pueden ejecutarse en paralelo una vez sus dependencias estan resueltas:

- **Despues de T02**: T03 y T04 (en paralelo)
- **Despues de T05**: T10, T16, T19, T22, T23 (en paralelo)
- **Despues de T07**: T08, T09, T12, T24, T25 (en paralelo)
- **Despues de T09**: T10, T11, T21 (en paralelo)
