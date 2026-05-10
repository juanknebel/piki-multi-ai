//! Unit tests for the pilot dialog input handlers in `input/dialog.rs`.
//! Covered: `ConfirmDelete`, `EditWorkspace`, `CommitMessage`. Acts as the
//! safety net before refactoring those handlers in phase 3 of the
//! quality-improvement plan.

use crossterm::event::{KeyCode, KeyModifiers};

use super::dialog::{
    handle_commit_message_input, handle_confirm_delete_input, handle_edit_workspace_input,
};
use crate::action::Action;
use crate::app::{ActivePane, App, AppMode};
use crate::dialog_state::{DialogState, EditWorkspaceField};
use crate::test_support::{key, key_with_mods, test_app};

// ── helpers ────────────────────────────────────────────────────────────────

fn open_confirm_delete(app: &mut App, target: usize) {
    app.mode = AppMode::ConfirmDelete;
    app.active_dialog = Some(DialogState::ConfirmDelete { target });
    app.active_pane = ActivePane::WorkspaceList;
}

fn open_edit_workspace(app: &mut App, active_field: EditWorkspaceField) {
    app.mode = AppMode::EditWorkspace;
    app.active_dialog = Some(DialogState::EditWorkspace {
        target: 0,
        kanban: String::new(),
        kanban_cursor: 0,
        prompt: String::new(),
        prompt_cursor: 0,
        group: String::new(),
        group_cursor: 0,
        active_field,
    });
}

fn open_commit_message(app: &mut App, buffer: &str) {
    app.mode = AppMode::CommitMessage;
    app.active_dialog = Some(DialogState::CommitMessage {
        buffer: buffer.to_string(),
    });
}

fn current_active_field(app: &App) -> EditWorkspaceField {
    match app.active_dialog {
        Some(DialogState::EditWorkspace { active_field, .. }) => active_field,
        _ => panic!("not in EditWorkspace dialog"),
    }
}

fn current_edit_buffers(app: &App) -> (String, String, String) {
    match &app.active_dialog {
        Some(DialogState::EditWorkspace {
            kanban,
            prompt,
            group,
            ..
        }) => (kanban.clone(), prompt.clone(), group.clone()),
        _ => panic!("not in EditWorkspace dialog"),
    }
}

fn current_edit_cursors(app: &App) -> (usize, usize, usize) {
    match app.active_dialog {
        Some(DialogState::EditWorkspace {
            kanban_cursor,
            prompt_cursor,
            group_cursor,
            ..
        }) => (kanban_cursor, prompt_cursor, group_cursor),
        _ => panic!("not in EditWorkspace dialog"),
    }
}

fn current_commit_buffer(app: &App) -> String {
    match &app.active_dialog {
        Some(DialogState::CommitMessage { buffer }) => buffer.clone(),
        _ => panic!("not in CommitMessage dialog"),
    }
}

// ── ConfirmDelete ─────────────────────────────────────────────────────────

#[test]
fn confirm_delete_yes_emits_delete_action_and_dismisses() {
    let mut app = test_app();
    open_confirm_delete(&mut app, 7);

    let action = handle_confirm_delete_input(&mut app, key(KeyCode::Char('y')));

    assert!(matches!(action, Some(Action::DeleteWorkspace(7, None))));
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.active_pane, ActivePane::WorkspaceList);
}

#[test]
fn confirm_delete_uppercase_y_also_confirms() {
    let mut app = test_app();
    open_confirm_delete(&mut app, 0);

    let action = handle_confirm_delete_input(&mut app, key(KeyCode::Char('Y')));

    assert!(matches!(action, Some(Action::DeleteWorkspace(0, None))));
    assert!(app.active_dialog.is_none());
}

#[test]
fn confirm_delete_no_emits_remove_from_list_and_dismisses() {
    let mut app = test_app();
    open_confirm_delete(&mut app, 3);

    let action = handle_confirm_delete_input(&mut app, key(KeyCode::Char('n')));

    assert!(matches!(action, Some(Action::RemoveFromList(3))));
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.active_pane, ActivePane::WorkspaceList);
}

#[test]
fn confirm_delete_esc_dismisses_without_action() {
    let mut app = test_app();
    open_confirm_delete(&mut app, 5);

    let action = handle_confirm_delete_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_delete_irrelevant_key_keeps_dialog_open() {
    let mut app = test_app();
    open_confirm_delete(&mut app, 0);

    let action = handle_confirm_delete_input(&mut app, key(KeyCode::Char('a')));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ConfirmDelete { target: 0 })
    ));
    assert_eq!(app.mode, AppMode::ConfirmDelete);
}

#[test]
fn confirm_delete_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    // No dialog set
    assert!(app.active_dialog.is_none());

    let action = handle_confirm_delete_input(&mut app, key(KeyCode::Char('y')));

    assert!(action.is_none());
}

// ── EditWorkspace ─────────────────────────────────────────────────────────

#[test]
fn edit_workspace_tab_cycles_kanban_to_prompt_to_group_to_kanban() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::KanbanPath);

    assert!(handle_edit_workspace_input(&mut app, key(KeyCode::Tab)).is_none());
    assert_eq!(current_active_field(&app), EditWorkspaceField::Prompt);

    handle_edit_workspace_input(&mut app, key(KeyCode::Tab));
    assert_eq!(current_active_field(&app), EditWorkspaceField::Group);

    handle_edit_workspace_input(&mut app, key(KeyCode::Tab));
    assert_eq!(current_active_field(&app), EditWorkspaceField::KanbanPath);
}

#[test]
fn edit_workspace_back_tab_cycles_in_reverse() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::KanbanPath);

    handle_edit_workspace_input(&mut app, key(KeyCode::BackTab));
    assert_eq!(current_active_field(&app), EditWorkspaceField::Group);

    handle_edit_workspace_input(&mut app, key(KeyCode::BackTab));
    assert_eq!(current_active_field(&app), EditWorkspaceField::Prompt);

    handle_edit_workspace_input(&mut app, key(KeyCode::BackTab));
    assert_eq!(current_active_field(&app), EditWorkspaceField::KanbanPath);
}

#[test]
fn edit_workspace_char_inserts_into_active_field_only() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::Prompt);

    for c in "hi".chars() {
        handle_edit_workspace_input(&mut app, key(KeyCode::Char(c)));
    }

    let (kanban, prompt, group) = current_edit_buffers(&app);
    assert_eq!(kanban, "");
    assert_eq!(prompt, "hi");
    assert_eq!(group, "");
    let (_, p_cur, _) = current_edit_cursors(&app);
    assert_eq!(p_cur, 2);
}

#[test]
fn edit_workspace_backspace_deletes_previous_char() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::Group);
    for c in "abc".chars() {
        handle_edit_workspace_input(&mut app, key(KeyCode::Char(c)));
    }

    handle_edit_workspace_input(&mut app, key(KeyCode::Backspace));

    let (_, _, group) = current_edit_buffers(&app);
    assert_eq!(group, "ab");
    let (_, _, g_cur) = current_edit_cursors(&app);
    assert_eq!(g_cur, 2);
}

#[test]
fn edit_workspace_backspace_at_cursor_zero_is_noop() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::Prompt);

    handle_edit_workspace_input(&mut app, key(KeyCode::Backspace));

    let (_, prompt, _) = current_edit_buffers(&app);
    assert_eq!(prompt, "");
    let (_, p_cur, _) = current_edit_cursors(&app);
    assert_eq!(p_cur, 0);
}

#[test]
fn edit_workspace_enter_with_empty_kanban_emits_none_kanban_path() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::Prompt);
    for c in "do something".chars() {
        handle_edit_workspace_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_edit_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::EditWorkspace(idx, kanban, prompt, group)) => {
            assert_eq!(idx, 0);
            assert!(kanban.is_none());
            assert_eq!(prompt, "do something");
            assert!(group.is_none());
        }
        _ => panic!("expected EditWorkspace action"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn edit_workspace_enter_trims_kanban_and_group() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::KanbanPath);
    for c in "  board.json  ".chars() {
        handle_edit_workspace_input(&mut app, key(KeyCode::Char(c)));
    }
    handle_edit_workspace_input(&mut app, key(KeyCode::Tab)); // → Prompt
    handle_edit_workspace_input(&mut app, key(KeyCode::Tab)); // → Group
    for c in "  team-a  ".chars() {
        handle_edit_workspace_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_edit_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::EditWorkspace(_, kanban, _, group)) => {
            assert_eq!(kanban.as_deref(), Some("board.json"));
            assert_eq!(group.as_deref(), Some("team-a"));
        }
        _ => panic!("expected EditWorkspace action"),
    }
}

#[test]
fn edit_workspace_esc_dismisses_without_action() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::Prompt);
    for c in "draft".chars() {
        handle_edit_workspace_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_edit_workspace_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn edit_workspace_ctrl_g_dismisses_like_esc() {
    let mut app = test_app();
    open_edit_workspace(&mut app, EditWorkspaceField::KanbanPath);

    let action = handle_edit_workspace_input(
        &mut app,
        key_with_mods(KeyCode::Char('g'), KeyModifiers::CONTROL),
    );

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn edit_workspace_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    // No dialog set
    let action = handle_edit_workspace_input(&mut app, key(KeyCode::Tab));
    assert!(action.is_none());
}

// ── CommitMessage ────────────────────────────────────────────────────────

#[test]
fn commit_message_char_inserts_and_backspace_deletes() {
    let mut app = test_app();
    open_commit_message(&mut app, "");

    for c in "fix:".chars() {
        handle_commit_message_input(&mut app, key(KeyCode::Char(c)));
    }
    assert_eq!(current_commit_buffer(&app), "fix:");

    handle_commit_message_input(&mut app, key(KeyCode::Backspace));
    assert_eq!(current_commit_buffer(&app), "fix");
}

#[test]
fn commit_message_enter_with_empty_buffer_rejects() {
    let mut app = test_app();
    open_commit_message(&mut app, "");

    let action = handle_commit_message_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    // Dialog still open, status_message set
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::CommitMessage { .. })
    ));
    assert_eq!(app.mode, AppMode::CommitMessage);
    assert!(app.status_message.is_some());
}

#[test]
fn commit_message_enter_with_content_emits_git_commit_and_dismisses() {
    let mut app = test_app();
    open_commit_message(&mut app, "feat: add tests");

    let action = handle_commit_message_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::GitCommit(msg)) => assert_eq!(msg, "feat: add tests"),
        _ => panic!("expected GitCommit"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn commit_message_esc_dismisses_without_action() {
    let mut app = test_app();
    open_commit_message(&mut app, "wip");

    let action = handle_commit_message_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn commit_message_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    // No dialog set
    let action = handle_commit_message_input(&mut app, key(KeyCode::Char('x')));
    assert!(action.is_none());
}
