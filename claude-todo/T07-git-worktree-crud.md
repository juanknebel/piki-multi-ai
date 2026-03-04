# T07 — Git worktree CRUD

**Status:** DONE
**Fase:** 2 — Workspace Management
**Bloquea:** T08, T09, T12
**Bloqueada por:** T06

## Descripcion

Implementar las operaciones de git worktree: crear, listar, y eliminar.
Los worktrees se crean en `.agent-multi/worktrees/<name>/`.

## Detalle tecnico

```rust
// workspace/manager.rs

pub struct WorkspaceManager {
    base_repo: PathBuf,     // repo raiz donde se ejecuta la app
    worktrees_dir: PathBuf, // .agent-multi/worktrees/
}

impl WorkspaceManager {
    pub fn new(base_repo: PathBuf) -> anyhow::Result<Self>;

    /// Crea un nuevo worktree con branch nueva desde HEAD
    /// git worktree add .agent-multi/worktrees/<name> -b agent-multi/<name>
    pub async fn create(&self, name: &str) -> anyhow::Result<Workspace>;

    /// Lista worktrees existentes
    /// git worktree list --porcelain
    pub async fn list(&self) -> anyhow::Result<Vec<Workspace>>;

    /// Elimina worktree y branch
    /// 1. Kill proceso claude si esta corriendo
    /// 2. git worktree remove .agent-multi/worktrees/<name>
    /// 3. git branch -D agent-multi/<name>
    pub async fn remove(&self, name: &str) -> anyhow::Result<()>;
}
```

### Comandos git

```bash
# Crear
mkdir -p .agent-multi/worktrees
git worktree add .agent-multi/worktrees/ws-1 -b agent-multi/ws-1

# Listar
git worktree list --porcelain

# Eliminar
git worktree remove .agent-multi/worktrees/ws-1
git branch -D agent-multi/ws-1
```

## Acceptance Criteria

- [ ] `WorkspaceManager::create()` crea worktree y retorna Workspace
- [ ] `WorkspaceManager::remove()` elimina worktree y branch
- [ ] `WorkspaceManager::list()` lista worktrees existentes
- [ ] Directorio `.agent-multi/worktrees/` se crea automaticamente
- [ ] Manejo de errores (worktree ya existe, branch ya existe, etc.)
