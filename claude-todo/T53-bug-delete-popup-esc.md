# T53 — Bug: Popup de borrar workspace usa Ctrl+g en vez de Esc para cancelar

**Status**: OPEN
**Blocked by**: —

## Problem

El popup de confirmacion para borrar un workspace usa `Ctrl+g` para cancelar. Todos los demas popups de la aplicacion usan `Esc` para cancelar. Esto es inconsistente y confuso para el usuario.

## Expected behavior

Presionar `Esc` cierra/cancela el popup de borrar workspace, igual que en el resto de popups.

## Actual behavior

El popup de borrar workspace requiere `Ctrl+g` para cancelar.

## Files to investigate

- `src/main.rs` — manejo de eventos de teclado en el popup de borrado
- `src/app.rs` — AppMode y transiciones del popup de borrado
