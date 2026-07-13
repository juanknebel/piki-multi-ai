# agent-multi (TUI crate)

Main application crate. Binary `piki-multi-ai`. Depends on `piki-core` and `piki-api-client`.

## Architecture pattern

Every user interaction follows: **Key event -> AppMode/DialogState -> Action -> State mutation -> Render**

1. `input/mod.rs` routes keys: modal `AppMode`s first, then the tmux-style prefix state machine (`app.input_state`), then the focused pane
2. Handler in `input/dialog.rs` or `input/interaction.rs` returns an `Option<Action>`
3. `action/mod.rs:execute_action()` routes the `Action` to its domain module's `handle()`, which performs async work and mutates `App`
4. `ui/layout.rs` routes `AppMode` to the right render function in `ui/`

## Keybinding model (tmux-style prefix)

There are no navigation/interaction modes. Keys always go to the focused pane (`app.active_pane`); the terminal gets full PTY passthrough. App-level actions live behind a **one-shot prefix** (default `Ctrl+G`, `keybindings.prefix_key` in config):

- `InputState { Normal, PrefixPending, TermScroll, Resize }` in `app.rs` — only consulted in `AppMode::Normal`; modal dialogs intercept input before it. `Resize` is a tmux `bind -r`-style repeat mode entered by the sidebar/split resize actions (bare resize chords keep repeating until Esc).
- `[keybindings.app]` (`config.rs::default_app()`) maps action names to `BindingValue`s (string or array). `"prefix-x"` strings fire after the prefix; other strings are direct chords checked before pane routing (`try_direct_app_binding`). Defaults are all prefix chords except `copy`/`paste`, which stay pane-scoped direct chords inside the terminal handler (`ctrl-shift-c`/`ctrl-shift-v`). Terminal search is `terminal_search` = `prefix-f` (moved off a direct `ctrl-shift-f` so a terminal emulator like ghostty can't swallow it).
- **Binding-choice rule — no Shift where avoidable.** Default app chords MUST prefer an **unshifted lowercase** key. Do NOT bind a new action to an uppercase letter (`prefix-N`) or any Shift-requiring chord — every letter action is deliberately lowercase (`s r b m v` etc.). If every mnemonic lowercase key is taken, reach for `prefix-ctrl-<key>` before a Shift chord (Ctrl is acceptable; Shift is what we avoid). A Shift-requiring symbol is allowed ONLY for an entrenched convention (`:` palette, `?` help) or when it also carries an unshifted alternate in the array (e.g. `sidebar_grow = ["prefix->", "prefix-."]`). `}`/`{` (workspace next/prev) are the one accepted exception, kept by explicit user choice.
- **`action_catalog.rs` is the single source of truth for every user-facing key.** The command palette, the which-key overlay, the `prefix-?` help browser and the README's prefix table all derive from it — do not hand-add to any of them. Two axes describe an entry:
  - `Context` — where the key is live. `Context::Global` is a prefix chord (or a direct app chord) and is the only context the palette and which-key show; every other context is a pane or overlay, and the help browser renders one section per context (order: `HELP_ORDER`).
  - `Keys` — where the key text comes from. `Keys::Bind(table, action)` resolves through a `[keybindings.*]` table and so follows the user's rebinds (**prefer this**); `Keys::Raw("ctrl-o")` is hardcoded in a handler but still platform-formatted (macOS Cmd); `Keys::Fixed` is a literal that isn't one keystroke ("Type", "Mouse drag", "0-5") and may contain `{prefix}`.
- **Adding an app action**: add the default to `default_app()` (following the binding-choice rule above), the name to `APP_ACTIONS` in `input/mod.rs`, an arm in `dispatch_app_action()` calling a helper in `input/app_actions.rs`, and an `app(...)` entry in `action_catalog.rs`. Mirror the default in the commented `config.example.toml` (repo root), and add a row to the README's prefix table between the `<!-- BEGIN:prefix-keys -->` markers. The `readme_parity` tests fail the build if the README drifts; `every_bind_resolves_to_a_real_binding` fails it if a `Keys::Bind` points at a binding that doesn't exist. `prefix 1..9` (tab jump) is hardcoded in `handle_prefix_key`, not configurable — it is a `global_fixed(...)` catalog entry so the help still lists it.
- **Adding a pane/overlay key**: add a `local(Context::X, ...)` entry. If the context is new, add it to `Context`, `Context::title()`, `HELP_ORDER` and (if a chord opens it) `Context::opened_by()`.
- **Pane/dialog-local keys** live in their own `[keybindings.*]` table (`scroll`, `agents`, `workspaces`, `markdown`, `about`, `workspace_info`, `dashboard`, `logs`, `fuzzy`, `editor`, `new_workspace`, `new_tab`). A table is only real if a `matches_<table>()` exists on `Config` **and the handler calls it** — they used to be declared and then ignored, so rebinding changed the help text and nothing else. Add new ones via the `matches_ctx` helper, which supplies the per-action default fallback (serde replaces an overridden table wholesale, so without it a partial override silently drops the keys the user didn't list). In the text-entry overlays these bindings must be checked *before* the key reaches the text buffer. `is_cancel()` (Esc or the prefix) always cancels a dialog on top of whatever `exit` says.
- `prefix prefix` sends a literal prefix byte to the PTY (`send_literal_prefix`), `Esc` cancels, unknown chords toast. Terminal scroll mode (`prefix [`) is `handle_term_scroll_key` + the `scroll` config context.
- Status bar shows `[PREFIX]`/`[SCROLL]` chips (`theme.status_bar.prefix_bg`); the focused pane border uses `theme.border.active`.
- **The Workspaces pane is follow-focus**: moving its cursor (j/k, wheel, click) calls `switch_workspace()`, so `selected_workspace` and `active_workspace` are always the same workspace. They used to drift — the cursor on one workspace, the main panel (and therefore `prefix c`'s new tab, which targets `active_workspace`) on another — which made every workspace-scoped action target whichever of the two the caller happened to read. Keep new sidebar-cursor movements going through `App::follow_sidebar_row()`; don't reintroduce a bare `selected_workspace = idx`.
- The bottom-left pane is **Agents** (`ActivePane::Agents`): `App::agent_rows()` lists (workspace, tab) pairs with Custom providers, plus any tab whose cli-agent channel has reported — shell tabs get the FIFO + hook env too (`spawn_tab` Shell arm) and the shell bridge wraps a manual `claude` with `--settings $PIKI_CLAUDE_HOOK_SETTINGS`, so it shows up as `Claude (Shell)` after its first hook event (and drops off when the CLI exits: the shell's OSC 133 command-end clears `ShellTabState.cli_agent` — a Custom-provider tab runs the agent directly, never sees command-end, and always lists). `render_agents_pane` (ui/sidebar.rs) derives status live via `cli_agent_snapshot()` (rich status for agents with a hook bridge — Claude Code, Antigravity) falling back to `agent_tab_indicator()` (bare PTY liveness: alive/exited) for everything else — if an agent shows `alive` instead of `running`/`idle`, its binary has no `piki_core::cli_agent::bridge_for_command()` entry; `handle_agents_interaction` + `jump_to_agent` (input/interaction.rs) handle j/k/Enter; the `agents` config context holds its local keys. The highlighted row tracks the tab the user is standing on: `App::sync_agent_selection()` runs once per event-loop iteration and re-selects only when the active tab changed (so j/k browsing isn't yanked back, and a non-agent tab leaves the highlight put) — don't sync `selected_agent_row` at the individual tab/workspace switch sites instead, they're too many to keep in step.

## Adding a new dialog/overlay

1. Add variant to `DialogState` in `dialog_state.rs`
2. Add variant to `AppMode` in `app.rs`
3. Add input handler `handle_<name>_input()` in `input/dialog.rs` — **reuse the helpers in `input/confirm_common.rs`, `input/text_field_common.rs`, `input/list_nav.rs` (see "Reusable dialog helpers" below)**
4. Add render function `render_<name>_dialog()` in `ui/dialogs.rs`
5. Wire input routing in `input/mod.rs` (match arm + import)
6. Wire render routing in `ui/layout.rs` (match arm)
7. If async work needed: add `Action` variant in `action/mod.rs` + arm in the matching domain module's `handle()` + routing arm in `execute_action()`
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
| `CycleFieldCtx<Ctx>` trait (in `dialog_state.rs`) | `next_ctx(&ctx)`/`prev_ctx(&ctx)` for cycling when the next/prev field depends on a runtime value (e.g. `DialogField` skipping `Name` when `WorkspaceType != Worktree`). Use this instead of `CycleField` when a field is conditionally hidden by another field in the same dialog. Implemented by `DialogField` with context `WorkspaceType`. |

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
- Use `matches!(action, Some(Action::Variant(_)))` to assert on actions. `Action` derives `Debug`, so `assert_eq!(format!("{action:?}"), ...)` works too when you need to compare payloads.
- Snapshot tests for rendering live in `ui/mod.rs` (`insta::assert_snapshot!` with `TestBackend`); they assert on the rendered buffer, not handler state.

## Key modules

- `app.rs` — `App` struct (centralized state), `AppMode`, `ActivePane`, `Workspace`
- `action/` — `Action` enum + `execute_action()` dispatch in `mod.rs`; per-domain `handle()` in `workspace.rs`, `files.rs`, `review.rs`, `tabs.rs`, `api.rs`, `chat.rs`, `agent.rs`. All git handling is delegated to the lazygit tab (`AIProvider::Git`, prefix g) — do NOT add native git actions back
- `dialog_state.rs` — `DialogState` enum with per-dialog data, `CycleField` trait, per-dialog field enums
- `event_loop.rs` — Main async loop at 50ms tick rate
- `input/` — Key routing + prefix dispatch (`mod.rs`), app action bodies (`app_actions.rs`), dialog handlers (`dialog.rs`), focused-pane handlers (`interaction.rs`), mouse (`mouse.rs`), text fields (`text_field_common.rs`), confirm helpers + `with_dialog_mut!` (`confirm_common.rs`), list navigation (`list_nav.rs`), input handler tests (`dialog_tests.rs`)
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
- **Config key added/renamed/removed in `config.rs`?** Also update `config.example.toml` at the repo root — it is a hand-maintained, fully-commented template that `scripts/install.sh` copies to the user's `config.toml` on install. It drifts silently from the code defaults otherwise (nothing enforces parity). Keep every option commented out (the app supplies defaults; the file only overrides), except the active `theme = "default"`.
