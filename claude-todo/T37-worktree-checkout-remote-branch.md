# T37 — Checkout remote branch when creating worktree

**Status**: DONE
**Bloqueada por**: —

## Contexto

Actualmente `WorkspaceManager::create()` siempre ejecuta `git worktree add <path> -b <branch>`, lo cual crea una rama nueva desde HEAD. Si la rama ya existe en el remoto (ej. `origin/<name>`), debería hacer checkout de esa rama en vez de crear una nueva.

## Lógica propuesta

En `src/workspace/manager.rs`, dentro de `create()`, antes de ejecutar `git worktree add`:

1. Ejecutar `git ls-remote --heads origin <branch_name>` para chequear si la rama existe en el remoto.
2. **Si NO existe en el remoto**: comportamiento actual — `git worktree add <path> -b <branch>` (crea rama nueva desde HEAD).
3. **Si SÍ existe en el remoto**:
   - Fetch la rama: `git fetch origin <branch_name>`
   - Crear el worktree haciendo checkout: `git worktree add <path> <branch_name>` (sin `-b`, usa la rama remota trackeada).
   - Si la rama local no existe pero la remota sí, git automáticamente crea el tracking branch.

## Archivos a modificar

- `src/workspace/manager.rs` — Modificar `create()` con la lógica de detección de rama remota.

## Verificación

- `cargo build`
- `cargo test`
- `cargo clippy`
- Test manual: crear workspace con nombre de rama que existe en remoto → debe hacer checkout de esa rama.
- Test manual: crear workspace con nombre nuevo → debe crear rama nueva (comportamiento actual).
