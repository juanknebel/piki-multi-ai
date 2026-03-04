# T30 — Worktrees en $HOME/.local/share/piki-multi/worktrees/<project>

**Status:** DONE
**Fase:** 9 — Mejoras
**Bloquea:** —
**Bloqueada por:** —

## Descripcion

Cambiar la ubicacion default de worktrees de `.agent-multi/worktrees/<name>` (dentro del repo) a `$HOME/.local/share/piki-multi/worktrees/<original_folder>/<name>` donde `<original_folder>` es el nombre del directorio raiz del proyecto.

## Acceptance Criteria

- [ ] Worktrees se crean en `$HOME/.local/share/piki-multi/worktrees/<project_dir>/<name>`
- [ ] Branch prefix cambia a `piki-multi/`
- [ ] Remove sigue funcionando con la nueva ubicacion
- [ ] cargo build compila
- [ ] cargo test pasa
