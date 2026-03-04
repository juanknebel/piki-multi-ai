# T03 — Main loop con crossterm event handling

**Status:** DONE
**Fase:** 1 — Core App Shell
**Bloquea:** T05, T08, T11, T19
**Bloqueada por:** T01, T02

## Descripcion

Implementar el loop principal de la app con:
- Inicializacion de terminal (crossterm enable_raw_mode, alternate screen)
- Loop de renderizado + polling de eventos
- Cleanup al salir (restore terminal)
- Integracion con tokio runtime

## Detalle tecnico

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Init terminal
    // 2. Create App state
    // 3. Loop:
    //    a. terminal.draw(|frame| ui::render(frame, &app))
    //    b. Poll crossterm events (con timeout ~50ms)
    //    c. Handle key events → update App state
    //    d. Poll tokio channels (PTY output, file events)
    //    e. Si app.should_quit → break
    // 4. Restore terminal
}
```

El tick rate debe ser ~50ms para que el PTY se sienta responsive.

## Acceptance Criteria

- [ ] App arranca, muestra terminal alternativa
- [ ] `q` cierra la app limpiamente
- [ ] Terminal se restaura correctamente al salir (incluso con panic)
- [ ] Tokio runtime funcionando
