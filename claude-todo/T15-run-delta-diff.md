# T15 — Ejecutar git diff | delta side-by-side

**Status:** DONE
**Fase:** 5 — Diff View
**Bloquea:** T16
**Bloqueada por:** T13

## Descripcion

Cuando el usuario presiona Enter en un archivo de la lista, ejecutar git diff piped
a delta con side-by-side y capturar el output ANSI para renderizarlo.

## Comando

```bash
git diff --color=always HEAD -- <file_path> \
  | delta --side-by-side \
          --width <panel_width> \
          --paging never \
          --true-color always \
          --line-fill-method ansi
```

Donde `<panel_width>` es el ancho del panel derecho en caracteres.

## Detalle tecnico

```rust
// diff/runner.rs

pub async fn run_diff(
    worktree_path: &Path,
    file_path: &str,
    width: u16,
) -> anyhow::Result<Vec<u8>> {
    // 1. Spawn git diff
    let git_diff = tokio::process::Command::new("git")
        .args([
            "diff", "--color=always", "HEAD", "--", file_path,
        ])
        .current_dir(worktree_path)
        .stdout(Stdio::piped())
        .spawn()?;

    // 2. Pipe a delta
    let delta = tokio::process::Command::new("delta")
        .args([
            "--side-by-side",
            &format!("--width={}", width),
            "--paging=never",
            "--true-color=always",
            "--line-fill-method=ansi",
        ])
        .stdin(git_diff.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .spawn()?;

    // 3. Capturar output
    let output = delta.wait_with_output().await?;
    Ok(output.stdout)
}
```

### Nota sobre el pipe

Se usan dos procesos encadenados. El stdout de git diff se conecta al stdin de delta.
Ambos son procesos async de tokio.

## Acceptance Criteria

- [x] git diff | delta se ejecuta correctamente
- [x] Output ANSI capturado como Vec<u8>
- [x] Width se pasa correctamente a delta
- [x] Manejo de caso "archivo sin cambios" (output vacio)
- [x] Manejo de errores (delta no instalado, archivo no existe)
- [x] No bloquea el main loop
