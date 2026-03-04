# T42 — Confirmar borrado de workspace y opción de mantener worktree

**Status:** DONE
**Phase:** 15 — UX Improvements
**Blocks:** —
**Blocked by:** —

## Description

Actualmente al presionar `d` para borrar un workspace, se elimina inmediatamente sin confirmación, incluyendo el worktree y el branch de git. Esto es peligroso — el usuario puede perder trabajo no commiteado.

Se necesita un diálogo de confirmación que pregunte:
1. Si realmente desea borrar el workspace
2. Si desea borrar también el worktree del filesystem o solo eliminarlo de la lista

### Flujo propuesto

1. Usuario presiona `d`
2. Aparece un overlay de confirmación:
   ```
   ┌─ Delete Workspace ─────────────────────┐
   │                                         │
   │  Delete workspace "feature/login"?      │
   │                                         │
   │  [y] Yes, delete worktree and branch    │
   │  [n] No, keep worktree on disk          │
   │  [Esc] Cancel                           │
   │                                         │
   └─────────────────────────────────────────┘
   ```
3. Si elige `y`: comportamiento actual (git worktree remove + branch -D)
4. Si elige `n`: solo elimina de la lista de la app, el worktree queda en disco
5. Si elige `Esc`: cancela, no hace nada

## Acceptance Criteria
- [ ] Al presionar `d` aparece un diálogo de confirmación
- [ ] Opción `y` borra el worktree y el branch (comportamiento actual)
- [ ] Opción `n` remueve el workspace de la app pero conserva el worktree en disco
- [ ] `Esc` cancela la operación
- [ ] El diálogo muestra el nombre del workspace a borrar
- [ ] Se persiste la configuración después de borrar
