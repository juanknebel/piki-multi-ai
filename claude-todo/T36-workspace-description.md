# T36 — Add description field to workspaces

**Status**: DONE
**Bloqueada por**: —

## Objetivo

Add a free-text `description` field to workspaces so users can remember what each workspace is about. The workspace name is restricted to alphanumeric/`-`/`_` (used as git branch/dir name), so a separate description field is needed.

## Changes

1. `src/app.rs` — Add `description` to `Workspace`, `DialogField::Description`, `desc_input_buffer` to `App`
2. `src/main.rs` — Tab cycles 3 fields, description input handling, pass description through actions
3. `src/workspace/manager.rs` — `create()` accepts description
4. `src/workspace/config.rs` — Persist description in `WorkspaceEntry`
5. `src/ui/layout.rs` — Third input field in dialog, show description in workspace list

## Verification

- `cargo build`
- `cargo test`
- `cargo clippy`
