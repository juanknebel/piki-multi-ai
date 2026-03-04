# T14 — Tracking de archivos cambiados via git diff

**Status:** DONE
**Fase:** 4 — File Watching
**Bloquea:** T13
**Bloqueada por:** T06, T12

## Descripcion

Cuando el file watcher detecta cambios (o periodicamente), ejecutar
`git diff --name-status HEAD` en el worktree para obtener la lista actualizada
de archivos cambiados con su status.

## Detalle tecnico

```rust
// Dentro de Workspace o como funcion en workspace/manager.rs

pub async fn get_changed_files(worktree_path: &Path) -> anyhow::Result<Vec<ChangedFile>> {
    let output = tokio::process::Command::new("git")
        .args(["diff", "--name-status", "HEAD"])
        .current_dir(worktree_path)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = stdout.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if parts.len() != 2 { return None; }
            let status = match parts[0] {
                "M" => FileStatus::Modified,
                "A" => FileStatus::Added,
                "D" => FileStatus::Deleted,
                s if s.starts_with("R") => FileStatus::Renamed,
                _ => return None,
            };
            Some(ChangedFile {
                path: parts[1].to_string(),
                status,
            })
        })
        .collect();
    Ok(files)
}
```

### Estrategia de actualizacion

- Cuando llega un WatchEvent, marcar el workspace como "dirty"
- En el main loop, si un workspace esta dirty y pasaron >500ms desde el ultimo refresh,
  ejecutar `get_changed_files()` y actualizar el estado
- Esto evita ejecutar git diff en cada evento individual del filesystem

## Acceptance Criteria

- [x] Parseo correcto de `git diff --name-status` output (parse_name_status in app.rs)
- [x] Maneja M, A, D, R correctamente
- [x] Debounce de 500ms en main loop (DEBOUNCE const + last_refresh tracking)
- [x] Funciona async sin bloquear el main loop (tokio::process::Command)
- [x] Tests unitarios para el parseo (4 tests in app::tests)
