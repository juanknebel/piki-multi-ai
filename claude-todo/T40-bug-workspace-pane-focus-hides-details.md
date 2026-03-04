# T40 — Bug: workspace list solo muestra nombre cuando tiene foco

**Status:** DONE
**Phase:** 14 — Bug Fixes
**Blocks:** —
**Blocked by:** —

## Description

Cuando el pane de workspaces está seleccionado/interactuando (tiene foco), solo se ve el nombre del workspace. Los datos adicionales (descripción, path, status) solo aparecen cuando se deja de hacer foco en el pane.

El problema probablemente está en el estilo de `selected_workspace` — el `bg(Color::DarkGray)` aplicado al `ListItem` cuando `is_active` oculta visualmente las líneas adicionales, o el alto del item no es suficiente para mostrar todas las líneas cuando hay highlight de selección.

## Steps to Reproduce

1. Abrir la aplicación con al menos un workspace
2. Navegar al pane de workspaces (panel izquierdo superior)
3. Hacer foco (Enter para interactuar)
4. Observar que solo se ve el nombre del workspace
5. Presionar Esc para salir del modo interacción
6. Observar que ahora se ven nombre, descripción, path y status

## Acceptance Criteria
- [ ] Cuando el pane de workspaces tiene foco, se muestran todos los datos de cada workspace (nombre, status, descripción, path)
- [ ] El highlight de selección se aplica correctamente a todas las líneas del item
