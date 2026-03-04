# T25 — Dialogo para crear nuevo workspace

**Status:** DONE
**Fase:** 7 — Polish
**Bloquea:** —
**Bloqueada por:** T07, T08

## Descripcion

Cuando el usuario presiona `n`, mostrar un mini-dialogo para ingresar el nombre
del nuevo workspace. Con ese nombre se crea el worktree y se lanza Claude Code.

## Detalle visual

```
┌─── New Workspace ────────────┐
│                               │
│  Name: my-feature█            │
│                               │
│  [Enter] Create  [Esc] Cancel │
└───────────────────────────────┘
```

## Detalle tecnico

Agregar un modo `AppMode::NewWorkspace` con un campo `input_buffer: String`.

```rust
AppMode::NewWorkspace => {
    match key.code {
        KeyCode::Char(c) => app.input_buffer.push(c),
        KeyCode::Backspace => { app.input_buffer.pop(); },
        KeyCode::Enter => {
            let name = app.input_buffer.drain(..).collect::<String>();
            if !name.is_empty() {
                // Crear workspace async
                let ws = manager.create(&name).await?;
                let pty = PtySession::spawn(&ws.path, rows, cols).await?;
                let watcher = FileWatcher::new(ws.path.clone(), ws.name.clone())?;
                // Agregar a app.workspaces
                app.switch_workspace(app.workspaces.len() - 1);
            }
            app.mode = AppMode::Normal;
        }
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}
```

## Acceptance Criteria

- [x] `n` abre el dialogo
- [x] Input de texto funciona (caracteres, backspace)
- [x] Enter crea el workspace y lanza Claude Code
- [x] Esc cancela sin crear nada
- [x] Validacion: nombre no vacio, sin espacios, sin caracteres especiales
- [x] Despues de crear, el nuevo workspace queda activo
