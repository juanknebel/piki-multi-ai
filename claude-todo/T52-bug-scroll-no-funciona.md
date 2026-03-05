# T52 — Bug: El scroll del terminal no funciona

**Status**: DONE
**Blocked by**: —
**Related**: T43

## Problem

T43 implemento scrollback en el panel del terminal, pero el scroll NO funciona en la practica. La parte 2 de T43 (scrollback con teclado y mouse) quedo incompleta o rota.

Segun T43, el scroll deberia funcionar con:
- `Shift+K` / `Shift+J` o `PageUp` / `PageDown` para scroll linea a linea o por pagina
- `ScrollUp` / `ScrollDown` del mouse para navegar el buffer de scrollback
- Auto-scroll al fondo con output nuevo
- Indicador visual cuando se esta viendo historial

## Expected behavior

El usuario puede hacer scroll hacia arriba en el terminal PTY para ver output previo, usando teclado o mouse.

## Actual behavior

El scroll no responde. No se puede navegar el historial del terminal.

## Files to investigate

- `src/main.rs` — manejo de eventos de scroll (keyboard + mouse)
- `src/app.rs` — estado de scroll por tab
- `src/ui/terminal.rs` — renderizado con scroll offset
- `src/pty/session.rs` — acceso al scrollback buffer de vt100
