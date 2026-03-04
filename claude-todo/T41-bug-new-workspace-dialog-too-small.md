# T41 — Bug: popup de nuevo workspace es muy pequeño y no tiene scroll horizontal

**Status:** OPEN
**Phase:** 14 — Bug Fixes
**Blocks:** —
**Blocked by:** —

## Description

El diálogo para crear un nuevo workspace (`n`) es demasiado pequeño. Cuando el texto ingresado en cualquier campo (Name, Dir, Desc) excede el ancho visible del popup, no hay forma de ver lo que se escribió — no hay scroll horizontal ni indicador de que el texto continúa fuera del área visible.

Actualmente el diálogo usa `centered_rect(50, 9, area)` que crea un popup fijo de 50 columnas × 9 filas.

## Problemas

1. **Popup muy pequeño** — 50 columnas no son suficientes para paths largos (ej: `~/projects/my-company/my-project/src`)
2. **Sin scroll horizontal** — Si el texto excede el ancho, se corta sin indicación visual
3. **Altura insuficiente** — 9 filas puede ser ajustado para el contenido actual

## Acceptance Criteria
- [ ] El popup es más grande (al menos 70-80% del ancho disponible)
- [ ] Si el texto excede el ancho del campo, se muestra la parte final del texto (scroll automático al cursor)
- [ ] El usuario puede ver qué está escribiendo en todo momento
- [ ] La altura del popup es adecuada para acomodar los tres campos cómodamente
