# T21 — Sesiones PTY concurrentes

**Status:** DONE
**Fase:** 6 — Tabs & Multi-workspace
**Bloquea:** —
**Bloqueada por:** T09, T20

## Descripcion

Asegurar que multiples instancias de Claude Code corren en paralelo,
cada una en su propio PTY, sin interferencia. El estado de cada terminal
se mantiene independientemente via su propio vt100::Parser.

## Detalle tecnico

### Arquitectura de concurrencia

```
Main Loop (tokio)
  │
  ├── Workspace 1
  │   ├── PtySession (reader task en spawn_blocking)
  │   ├── vt100::Parser (Arc<Mutex<>>)
  │   └── FileWatcher (notify thread)
  │
  ├── Workspace 2
  │   ├── PtySession (reader task en spawn_blocking)
  │   ├── vt100::Parser (Arc<Mutex<>>)
  │   └── FileWatcher (notify thread)
  │
  └── Workspace N
      └── ...
```

### Consideraciones de memoria

- Cada vt100::Parser con 1000 lineas de scrollback: ~200KB
- Cada PTY reader: 4KB buffer
- Para 10 workspaces: ~2MB total (negligible)

### Sincronizacion

- Los Mutex sobre vt100::Parser se lockean brevemente:
  - Writer (spawn_blocking thread): lock para process(bytes)
  - Renderer (main loop): lock para screen()
  - Contention minima porque el render es cada 50ms

### Lifecycle

```
create_workspace():
  1. git worktree add
  2. PtySession::spawn(worktree_path)
  3. FileWatcher::new(worktree_path)
  4. Push workspace al Vec

remove_workspace():
  1. PtySession::kill()
  2. Drop FileWatcher
  3. git worktree remove
  4. Remove del Vec
  5. Ajustar active_workspace index
```

## Acceptance Criteria

- [x] 3+ workspaces pueden correr Claude Code simultaneamente
- [x] Cada PTY es independiente (input de uno no afecta otro)
- [x] Switching entre workspaces muestra estado correcto de cada terminal
- [x] Crear/eliminar workspaces no afecta a los demas
- [x] No hay deadlocks ni data races
