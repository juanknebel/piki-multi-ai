# T24 — Cleanup al salir

**Status:** DONE
**Fase:** 7 — Polish
**Bloquea:** —
**Bloqueada por:** T07, T09

## Descripcion

Cuando la app se cierra (q o Ctrl+C), limpiar todos los recursos:
- Matar todos los procesos de Claude Code
- Preguntar si eliminar los worktrees o conservarlos
- Restaurar la terminal correctamente (incluso en caso de panic)

## Detalle tecnico

### Panic handler

```rust
// En main, antes de init terminal:
let original_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |panic_info| {
    // Restaurar terminal
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen
    );
    original_hook(panic_info);
}));
```

### Shutdown sequence

```rust
async fn shutdown(app: &mut App) -> anyhow::Result<()> {
    // 1. Matar todos los procesos PTY
    for ws in &mut app.workspaces {
        if let Some(ref mut pty) = ws.pty_session {
            let _ = pty.kill();
        }
    }

    // 2. Opcionalmente preguntar sobre worktrees
    // Para POC: solo matar procesos, dejar worktrees
    // Los worktrees se pueden limpiar con:
    //   git worktree list
    //   git worktree remove <path>

    Ok(())
}
```

### Signal handling

Capturar SIGINT (Ctrl+C) y SIGTERM para cleanup:

```rust
let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())?;
tokio::select! {
    _ = sigint.recv() => { shutdown(&mut app).await?; }
    // ... main loop
}
```

## Acceptance Criteria

- [x] Todos los procesos de Claude Code se matan al salir
- [x] Terminal se restaura correctamente
- [x] Panic handler restaura terminal
- [x] Ctrl+C hace cleanup antes de salir
- [x] No quedan procesos zombie
