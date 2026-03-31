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
3. Add input handler `handle_<name>_input()` in `input/dialog.rs`
4. Add render function `render_<name>_dialog()` in `ui/dialogs.rs`
5. Wire input routing in `input/mod.rs` (match arm + import)
6. Wire render routing in `ui/layout.rs` (match arm)
7. If async work needed: add `Action` variant in `action.rs` + handler in `execute_action()`

## Key modules

- `app.rs` — `App` struct (centralized state), `AppMode`, `ActivePane`, `Workspace`
- `action.rs` — `Action` enum and async `execute_action()` handler
- `dialog_state.rs` — `DialogState` enum with per-dialog data
- `event_loop.rs` — Main async loop at 50ms tick rate
- `input/` — Key routing (`mod.rs`), dialog handlers (`dialog.rs`), interaction mode (`interaction.rs`), mouse (`mouse.rs`), text fields (`text_field_common.rs`)
- `ui/` — Rendering: `layout.rs` (compositor), `dialogs.rs` (overlays), component sub-modules
- `config.rs` — Keybindings and settings from `config.toml`
- `theme.rs` — Color theme loading
- `syntax.rs` — `syntect` wrapper for syntax highlighting

## Conventions

- Render functions are pure: `fn(frame, area, &App)` — no side effects.
- Use `StatefulWidget` when a widget needs scroll state or selection.
- Prefer `Line::from(vec![spans...])` for styled text.
- Use `clear_popup()` + `popup_block()` for overlay dialogs.
- Snapshot test new UI with `insta::assert_snapshot!` using `TestBackend` (tests in `ui/mod.rs`, snapshots in `ui/snapshots/`).
- Mouse hit-testing: store `Rect` areas on `App`, check in `mouse.rs`.
- Footer hints: context-sensitive per `AppMode`; cache key must reflect state transitions.
