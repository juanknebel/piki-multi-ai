# T28 — Reemplazar git diff por git status --porcelain en Changed Files

**Status:** DONE
**Fase:** 9 — Mejoras
**Bloquea:** —
**Bloqueada por:** T14

## Descripcion

El panel "CHANGED FILES" actualmente solo usa `git diff --name-status HEAD`, lo cual no detecta archivos untracked, conflictos de merge, ni distingue staged/unstaged. Reemplazar por `git status --porcelain=v1` para cobertura completa.

## Acceptance Criteria

- [ ] FileStatus enum ampliado con Untracked, Conflicted, Staged, StagedModified
- [ ] get_changed_files usa `git status --porcelain=v1`
- [ ] Nuevo parser parse_porcelain_status reemplaza parse_name_status
- [ ] UI muestra colores/labels para todos los estados
- [ ] Tests cubren todos los estados incluyendo untracked, conflictos, renamed
- [ ] Diff runner maneja archivos untracked con --no-index
- [ ] cargo test pasa
- [ ] cargo clippy sin warnings nuevos
- [ ] cargo build compila
