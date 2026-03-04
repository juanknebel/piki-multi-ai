# T01 — Inicializar proyecto Cargo con dependencias

**Status:** DONE
**Fase:** 0 — Setup
**Bloquea:** T02, T03, T04, T05
**Bloqueada por:** —

## Descripcion

Crear el proyecto Rust con `cargo init` y configurar todas las dependencias en `Cargo.toml`.

## Acceptance Criteria

- [ ] `cargo init` ejecutado en `/Users/jknebel/git/agent-multi`
- [ ] `Cargo.toml` con todas las dependencias:
  - ratatui (features: crossterm)
  - crossterm
  - tokio (features: full)
  - portable-pty
  - vt100
  - tui-term
  - ansi-to-tui
  - notify
  - serde + serde_json
  - anyhow
- [ ] `cargo check` compila sin errores
