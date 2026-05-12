# agent-multi (TUI crate)

Main application crate. Binary `piki-multi-ai`. Depends on `piki-core` and `piki-api-client`.

## Architecture pattern

Every user interaction follows: **Key event -> AppMode/DialogState -> Action -> State mutation -> Render**

1. `input/mod.rs` routes keys by `AppMode` to the right handler
2. Handler in `input/dialog.rs` or `input/interaction.rs` returns an `Option<Action>`
3. `action.rs:execute_action()` performs async work and mutates `App`
4. `ui/layout.rs` routes `AppMode` to the right render function in `ui/`

## Adding a new dialog/overlay

1. Add variant to `DialogState` in `dialog_state.rs`
2. Add variant to `AppMode` in `app.rs`
3. Add input handler `handle_<name>_input()` in `input/dialog.rs` — **reuse the helpers in `input/confirm_common.rs`, `input/text_field_common.rs`, `input/list_nav.rs` (see "Reusable dialog helpers" below)**
4. Add render function `render_<name>_dialog()` in `ui/dialogs.rs`
5. Wire input routing in `input/mod.rs` (match arm + import)
6. Wire render routing in `ui/layout.rs` (match arm)
7. If async work needed: add `Action` variant in `action.rs` + handler in `execute_action()`
8. Add unit tests in `input/dialog_tests.rs` using the helpers in `crate::test_support` (see "Testing dialog handlers" below)

## Reusable dialog helpers

When implementing a new dialog input handler, prefer these instead of inlining boilerplate. They live in `input/confirm_common.rs`, `input/text_field_common.rs`, `input/list_nav.rs`, and `dialog_state.rs`.

| Helper | When to use |
|---|---|
| `dismiss_dialog(app)` | Standard cleanup: clears `active_dialog`, sets `mode = AppMode::Normal`. Use on Esc/cancel/submit unless the dialog transitions to another mode. |
| `dismiss_dialog_to_pane(app, pane)` | Same as above + sets `active_pane`. Used by dialogs that should drop focus on a specific pane (e.g. WorkspaceList after deleting a workspace). |
| `handle_yn_input(key) -> ConfirmResult` | Decodes y/Y/n/N/Esc into Yes/No/Cancel/NotHandled. Use in any confirm-Y/N dialog. |
| `with_dialog_mut!(app, Variant { fields } => { body })` | Destructures `app.active_dialog` by ref-mut, runs the body, returns `None` on mismatch. Replaces `let Some(DialogState::X { ref mut ... }) = app.active_dialog else { return None; };` boilerplate. The body returns `Option<Action>`. |
| `handle_text_input(buf, cursor, key, validator)` | Common text field editing: Char/Backspace/Delete/Left/Right/Home/End. ASCII fast path + Unicode-safe. |
| `is_cancel(key, platform)` | Returns true for Esc and Ctrl+G (platform-aware). Use in handlers that accept both. |
| `move_selection(selected: &mut usize, total, delta, wrap)` | Move a list cursor with clamp or wrap-around. Empty lists (`total==0`) are a no-op (no panic). Use for j/k/Up/Down arms in list-style dialogs. |
| `CycleField` trait (in `dialog_state.rs`) | `next()`/`prev()` for cycling between fields on Tab/BackTab. Implemented by `EditWorkspaceField`, `EditAgentField`, `EditProviderField`. Add a per-dialog enum + impl when introducing a new multi-field dialog. |

The borrow checker around `with_dialog_mut!` + `dismiss_dialog(app)` works because NLL releases the `&mut app.active_dialog` borrow once the field bindings stop being used. If you need to mutate other `app` fields *and* keep using the bindings, copy values out into locals first, then call `dismiss_dialog`. See `handle_edit_workspace_input` for an example using a local `Step { Stay, Cancel, Submit(Box<Action>) }` enum to defer the dismiss outside the macro scope.

## Testing dialog handlers

Input-handler tests live in `input/dialog_tests.rs`. They use the shared test infrastructure in `crate::test_support`:

- `test_app()` — fresh `App` with default config + JSON storage + default paths.
- `test_storage()` — bare `AppStorage` for cases that don't need an `App`.
- `test_terminal(w, h)` — `ratatui` `Terminal<TestBackend>` for rendering snapshots.
- `buffer_to_snapshot(buf)` — buffer → trimmed string for `insta::assert_snapshot!`.
- `key(KeyCode)` / `key_with_mods(KeyCode, KeyModifiers)` — `KeyEvent` helpers.

Conventions:
- Per dialog, write an `open_<name>(app, ...)` helper that sets both `app.mode` and `app.active_dialog`.
- Per dialog, write small accessor helpers (`current_<thing>(app)`) that pattern-match the dialog state and panic if absent — keeps assertions terse.
- Cover: each key path (navigation, action, submit, cancel), edge cases (empty list, no-dialog-active returns None), and any sub-mode transitions.
- For state requiring app population: `app.agent_profiles` is a plain Vec — push mocks directly. `app.log_buffer.lock().push_back(LogEntry { ... })` for log entries. Don't try to populate `app.workspaces` — `Workspace` requires PTY/git setup and isn't unit-testable.
- Use `matches!(action, Some(Action::Variant(_)))` to assert on actions (Action doesn't `derive(Debug)`).
- Snapshot tests for rendering live in `ui/mod.rs` (`insta::assert_snapshot!` with `TestBackend`); they assert on the rendered buffer, not handler state.

## Key modules

- `app.rs` — `App` struct (centralized state), `AppMode`, `ActivePane`, `Workspace`
- `action.rs` — `Action` enum and async `execute_action()` handler
- `dialog_state.rs` — `DialogState` enum with per-dialog data, `CycleField` trait, per-dialog field enums
- `event_loop.rs` — Main async loop at 50ms tick rate
- `input/` — Key routing (`mod.rs`), dialog handlers (`dialog.rs`), interaction mode (`interaction.rs`), mouse (`mouse.rs`), text fields (`text_field_common.rs`), confirm helpers + `with_dialog_mut!` (`confirm_common.rs`), list navigation (`list_nav.rs`), input handler tests (`dialog_tests.rs`)
- `ui/` — Rendering: `layout.rs` (compositor), `dialogs.rs` (overlays), component sub-modules
- `config.rs` — Keybindings and settings from `config.toml`
- `theme.rs` — Color theme loading
- `syntax.rs` — `syntect` wrapper for syntax highlighting
- `test_support.rs` — Test-only helpers (`#[cfg(test)]`) shared between rendering snapshot tests and input handler tests

## Conventions

- Render functions are pure: `fn(frame, area, &App)` — no side effects.
- Use `StatefulWidget` when a widget needs scroll state or selection.
- Prefer `Line::from(vec![spans...])` for styled text.
- Use `clear_popup()` + `popup_block()` for overlay dialogs.
- Snapshot test new UI with `insta::assert_snapshot!` using `TestBackend` (tests in `ui/mod.rs`, snapshots in `ui/snapshots/`).
- Mouse hit-testing: store `Rect` areas on `App`, check in `mouse.rs`.
- Footer hints: context-sensitive per `AppMode`; cache key must reflect state transitions.
