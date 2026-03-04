# T09 — Spawn Claude Code en PTY

**Status:** DONE
**Fase:** 3 — PTY Integration
**Bloquea:** T10, T11
**Bloqueada por:** T07

## Descripcion

Lanzar Claude Code como proceso hijo dentro de un pseudo-terminal (PTY) para cada workspace.
Esto permite capturar todo el output con colores ANSI y enviar input interactivo.

## Detalle tecnico

```rust
// pty/session.rs

pub struct PtySession {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    reader_handle: tokio::task::JoinHandle<()>,
}

impl PtySession {
    /// Spawn claude en un PTY dentro del directorio del worktree
    pub async fn spawn(
        worktree_path: &Path,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<Self> {
        // 1. Crear PTY con portable_pty::native_pty_system()
        // 2. PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
        // 3. CommandBuilder::new("claude")
        //    - Set cwd al worktree_path
        //    - Args: ["--dangerously-skip-permissions"] (para POC)
        // 4. pair.slave.spawn_command(cmd)
        // 5. pair.master.try_clone_reader() → reader
        // 6. pair.master.take_writer() → writer
        // 7. Crear vt100::Parser::new(rows, cols, 1000) // 1000 lines scrollback
        // 8. Spawn tokio task que lee del reader en loop y alimenta el parser
    }

    /// Enviar input al PTY (keystrokes del usuario)
    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<()>;

    /// Obtener referencia al parser para renderizar
    pub fn parser(&self) -> &Arc<Mutex<vt100::Parser>>;

    /// Resize del PTY cuando cambia el tamano del panel
    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()>;

    /// Matar el proceso
    pub fn kill(&mut self) -> anyhow::Result<()>;

    /// Verificar si el proceso termino
    pub fn is_alive(&self) -> bool;
}
```

### Reader task (tokio)

```rust
// El reader es sync (portable-pty), se wrappea con spawn_blocking
tokio::task::spawn_blocking(move || {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,  // EOF
            Ok(n) => {
                let mut parser = parser_clone.lock().unwrap();
                parser.process(&buf[..n]);
            }
            Err(_) => break,
        }
    }
});
```

## Acceptance Criteria

- [x] Claude Code se lanza dentro del worktree
- [x] Output del PTY se alimenta al vt100::Parser
- [x] El parser acumula el estado de la terminal correctamente
- [x] Se puede enviar input (write bytes) al PTY
- [x] El proceso se puede matar limpiamente (kill + Drop impl)
- [x] Resize del PTY funciona (via master.resize + parser screen_mut().set_size)
