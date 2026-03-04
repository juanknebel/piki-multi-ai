# T20 — Switch entre workspaces

**Status:** DONE
**Fase:** 6 — Tabs & Multi-workspace
**Bloquea:** T21
**Bloqueada por:** T19

## Descripcion

Implementar la logica para cambiar entre workspaces activos. Al cambiar,
el panel derecho muestra el PTY del nuevo workspace y el panel de archivos
se actualiza.

## Keybindings

- `Tab`: Siguiente workspace
- `Shift+Tab`: Workspace anterior
- `1-9`: Ir directo al workspace N
- Seleccionar en la lista izquierda + `Enter`: Activar workspace

## Detalle tecnico

```rust
impl App {
    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.workspaces.len() {
            self.active_workspace = index;
            self.selected_file = 0;     // reset file selection
            self.mode = AppMode::Normal; // volver a PTY mode
            self.diff_content = None;    // limpiar diff
        }
    }

    pub fn next_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.switch_workspace((self.active_workspace + 1) % self.workspaces.len());
        }
    }

    pub fn prev_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            let len = self.workspaces.len();
            self.switch_workspace((self.active_workspace + len - 1) % len);
        }
    }
}
```

### Consideraciones

- El PTY del workspace anterior sigue corriendo en background
- El switch es instantaneo (solo cambia que parser se renderiza)
- Los archivos cambiados se muestran del workspace activo

## Acceptance Criteria

- [x] Tab/Shift+Tab cicla entre workspaces
- [x] 1-9 salta directo al workspace N
- [x] Panel derecho muestra PTY del workspace activo
- [x] Panel de archivos muestra archivos del workspace activo
- [x] Tab bar refleja el cambio de workspace activo
- [x] Modo diff se limpia al cambiar de workspace
