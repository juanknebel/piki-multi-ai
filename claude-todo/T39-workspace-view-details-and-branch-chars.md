# T39 — Mostrar descripción y path en lista de workspaces + caracteres de branch en nombre

**Status:** DONE
**Phase:** 14 — Bug Fixes
**Blocks:** —
**Blocked by:** —

## Description

La vista de workspaces en el panel izquierdo solo muestra el nombre.
Se necesita:
1. Mostrar también la **descripción** y el **path** del worktree en la lista.
2. Permitir el carácter `/` y otros caracteres válidos de branch de git en el campo Name del diálogo de nuevo workspace.

Caracteres válidos para branches git (según `git check-ref-format`):
- Alfanuméricos, `-`, `_`, `.`, `/`
- No puede empezar ni terminar con `.`
- No puede contener `..`, `@{`, espacios, `~`, `^`, `:`, `\`, `?`, `*`, `[`
- No puede terminar con `.lock`

## Acceptance Criteria
- [ ] La lista de workspaces muestra nombre, descripción (si existe) y path
- [ ] El campo Name del diálogo acepta `/`, `.` además de alfanuméricos, `-`, `_`
- [ ] El worktree_path se genera correctamente para nombres con `/` (reemplazando `/` por `_` en el path del filesystem)
- [ ] Compila sin errores
