# T06 — Workspace model y estado

**Status:** DONE
**Fase:** 2 — Workspace Management
**Bloquea:** T07, T08
**Bloqueada por:** T04

## Descripcion

Implementar el modelo de Workspace como struct independiente con su logica de estado.

## Detalle

El Workspace representa una sesion aislada con su propio worktree, branch, proceso de
Claude Code, y lista de archivos cambiados.

```rust
impl Workspace {
    pub fn new(name: String, branch: String, path: PathBuf) -> Self;
    pub fn file_count(&self) -> usize;
    pub fn status_label(&self) -> &str;  // "idle", "busy", "done", "error"
    pub fn add_changed_file(&mut self, file: ChangedFile);
    pub fn clear_changed_files(&mut self);
    pub fn refresh_changed_files(&mut self) -> anyhow::Result<()>;
    // refresh_changed_files ejecuta: git diff --name-status HEAD
    // y parsea el output para actualizar self.changed_files
}
```

## Acceptance Criteria

- [ ] Struct `Workspace` con todos los campos necesarios
- [ ] Metodo `refresh_changed_files` que ejecuta `git diff --name-status HEAD` y parsea output
- [ ] Tests unitarios para el parseo de `git diff --name-status`
