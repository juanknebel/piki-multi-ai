# T38 — Bug: Diff view hides provider shell pane

**Status**: DONE
**Bloqueada por**: —

## Descripción del bug

Al abrir la vista de diff (`git diff`), el panel principal muestra solo el diff y ya no se pueden ver los sub-tabs de providers (Claude, Gemini, Codex) ni la shell/PTY asociada. No hay forma de volver a ver el pane de las shells mientras se está en modo diff.

## Comportamiento esperado

Al cambiar entre la vista diff y la vista terminal, los sub-tabs de providers deberían seguir visibles o debería ser fácil alternar entre ambas vistas sin perder acceso a las shells.

## Archivos relevantes

- `src/ui/layout.rs` — Renderizado del panel principal (diff vs terminal)
- `src/main.rs` — Toggle entre modo diff y terminal (Action::OpenDiff)
- `src/app.rs` — Estado de AppMode (Normal vs Diff)

## Verificación

- Abrir un workspace con archivos modificados
- Abrir la vista diff
- Verificar que los sub-tabs de providers siguen accesibles
- Verificar que se puede volver a la shell sin problemas
