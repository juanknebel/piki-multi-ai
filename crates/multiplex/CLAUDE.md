# piki-multiplex

A terminal multiplexer engine, independent of the rest of this workspace —
**must NOT depend on `piki-core`, `piki-tui`, `piki-desktop`, `piki-agent`, or
`piki-api-client`.** `piki-core` depends on this crate and re-exports its
`pty`/`session`/`shell_integration`/`cli_agent` modules unchanged (see
`crates/core/CLAUDE.md`), so downstream `piki_core::pty::*` /
`piki_core::session::*` call sites in the TUI and desktop never reference this
crate by name.

## Modules

- `pty/` — `PtySession` wraps `portable-pty` (sync) with `spawn_blocking` for
  non-blocking reads. `vt100::Parser` accumulates terminal state. The reader
  optionally feeds bytes through `shell_integration::parser::OscParser` and
  updates a per-session `ShellSession`. The vendored vt100 (workspace-root
  `[patch.crates-io]`, `vendor/vt100`) queues terminal-query answerbacks (DSR,
  DA1) in `Screen::take_answerback()`; the reader drains that after each parse
  pass. **All writes to the master go through a per-session `pty-writer`
  thread** (`PtySession::write` only enqueues) — a direct `write(2)` blocks
  once the kernel's PTY input queue fills, which would freeze the caller.
  `PtySession` is a `Local | Remote` facade (`RemotePty` backs it when
  attached to the session daemon over a Unix socket); the public API is
  identical either way.
- `session/` — the persistent-session daemon (design: `docs/persistent-sessions.md`
  in the workspace root). `protocol.rs` is the daemon⇄client wire format:
  `[kind u8][len u32 LE][payload]` frames, JSON for control messages, raw
  bytes for `Input`/`Output`/`Restore`; every JSON field is `#[serde(default)]`
  so additive changes stay compatible — bump `PROTOCOL_VERSION` for anything
  else. `restore.rs` wraps the vendored vt100's `Screen::restore_formatted()`
  (scrollback + screen + alt screen + modes) and owns its round-trip tests —
  keep them green when touching `vendor/vt100`. `daemon/launch.rs::run` takes
  a `DaemonPaths { sessions_dir, log_dir, lock_path, pid_path, socket_path,
  log_path }` — the embedding app builds this from its own paths type
  (`piki-core`'s `DataPaths::daemon_paths()`); the daemon itself never decides
  which binary it runs as — the caller re-execs itself with a `serve`
  subcommand / `--serve-sessions` flag and passes the resulting `DaemonPaths`.
  `sessions_enabled(config_path)` is a minimal `[sessions].enabled` TOML
  lookup (default `true` on any failure) — a generic file-layer helper, not
  tied to any app-specific settings merge.
- `shell_integration/` — init scripts (zsh, bash, fish) embedded via
  `include_str!`, `OscParser` (streaming OSC 133/7 state machine), an
  `install` module that detects the shell family and prepares env vars +
  extra args (`ZDOTDIR` / `--rcfile`) so the user's shell sources the bridge
  before its real dotfiles. Linux/macOS only. The OSC 777 arm
  (`parser.rs::parse_osc_777`) only claims sequences whose target matches
  `cli_agent::CLI_AGENT_TARGET`.
- `cli_agent/` — an optional, opinionated out-of-band channel: FIFO transport
  (`sock.rs`, per-tab, `PIKI_CLI_AGENT_SOCK`) plus one built-in event
  vocabulary (`CliAgentEvent`/`CliAgentState`/`CliAgentStatus`,
  `parse_cli_agent_payload`) shaped for Claude Code's hook JSON. This is
  genuinely piki-specific naming living inside an otherwise generic crate —
  a fully generic sidecar-event abstraction would need to reach into every
  frontend's rendering code (TUI, desktop Rust, and the desktop's TypeScript
  layer) to stay behavior-preserving, which was out of scope for the initial
  extraction from `piki-core`. A consumer that never passes a
  `cli_agent_sock` path at spawn time never sees any of these types.
  Hook-script *installation* (`install.rs`, `install_antigravity.rs`,
  `AgentBridge`, `bridge_for_command`) stays in `crates/core/src/cli_agent/`
  — it needs `DataPaths`/`ProviderManager`, which this crate must not depend
  on — and re-exports this module's types unchanged.

## Conventions

- Error handling: `anyhow::Result` for fallible operations.
- Thread safety: `parking_lot::Mutex` for shared state, no `unwrap()` on
  locks held across an `await`.
- Wire-protocol changes: keep new JSON fields `#[serde(default)]`; bump
  `session::protocol::PROTOCOL_VERSION` only for a genuinely breaking change.
