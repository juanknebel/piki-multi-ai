//! Unit tests for the dialog input handlers in `input/dialog.rs`.
//! Covered: all confirmations (`ConfirmDelete`, `ConfirmCloseTab`, `ConfirmQuit`,
//! `ConfirmMerge`), text dialogs (`CommitMessage`, `EditWorkspace`), scroll
//! overlays (`Help`, `About`, `WorkspaceInfo`), and list-navigation dialogs
//! (`DispatchCardMove`, `GitLog`, `Dashboard`, `Logs`, `GitStash`,
//! `ImportAgents`, `ManageAgents`, `ManageProviders`).

use crossterm::event::{KeyCode, KeyModifiers};
use piki_core::MergeStrategy;
use piki_core::storage::AgentProfile;

use super::dialog::{
    handle_about_input, handle_commit_message_input, handle_confirm_close_tab_input,
    handle_confirm_delete_input, handle_confirm_merge_input, handle_confirm_quit_input,
    handle_dashboard_input, handle_dispatch_card_move_input, handle_edit_provider_input,
    handle_edit_workspace_input, handle_git_log_input, handle_git_stash_input, handle_help_input,
    handle_import_agents_input, handle_logs_input, handle_manage_agents_input,
    handle_manage_providers_input, handle_new_tab_input, handle_workspace_info_input,
};
use crate::action::Action;
use crate::app::{ActivePane, App, AppMode};
use crate::dialog_state::{
    DialogState, EditProviderField, EditWorkspaceField, GitLogEntry, NewTabMenu,
};
use piki_core::AIProvider;
use crate::log_buffer::LogEntry;
use crate::test_support::{key, key_with_mods, test_app, test_app_isolated};

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

fn open_confirm_close_tab(app: &mut App, target: usize) {
    app.mode = AppMode::ConfirmCloseTab;
    app.active_dialog = Some(DialogState::ConfirmCloseTab { target });
}

fn open_confirm_quit(app: &mut App) {
    app.mode = AppMode::ConfirmQuit;
    app.active_dialog = Some(DialogState::ConfirmQuit);
}

fn open_confirm_merge(app: &mut App) {
    app.mode = AppMode::ConfirmMerge;
    app.active_dialog = Some(DialogState::ConfirmMerge);
}

fn open_help(app: &mut App, scroll: u16) {
    app.mode = AppMode::Help;
    app.active_dialog = Some(DialogState::Help { scroll });
}

fn open_about(app: &mut App) {
    app.mode = AppMode::About;
    app.active_dialog = Some(DialogState::About);
}

fn open_workspace_info(app: &mut App, hscroll: u16) {
    app.mode = AppMode::WorkspaceInfo;
    app.active_dialog = Some(DialogState::WorkspaceInfo { hscroll });
}

fn current_help_scroll(app: &App) -> u16 {
    match app.active_dialog {
        Some(DialogState::Help { scroll }) => scroll,
        _ => panic!("not in Help dialog"),
    }
}

fn current_workspace_info_hscroll(app: &App) -> u16 {
    match app.active_dialog {
        Some(DialogState::WorkspaceInfo { hscroll }) => hscroll,
        _ => panic!("not in WorkspaceInfo dialog"),
    }
}

fn open_dispatch_card_move(app: &mut App, target: usize, columns: Vec<(String, String)>) {
    app.mode = AppMode::DispatchCardMove;
    app.active_dialog = Some(DialogState::DispatchCardMove {
        target,
        columns,
        selected: 0,
    });
}

fn open_git_log(app: &mut App, lines: Vec<GitLogEntry>) {
    app.mode = AppMode::GitLog;
    app.active_dialog = Some(DialogState::GitLog {
        lines,
        selected: 0,
        scroll: 0,
    });
}

fn current_git_log_selected(app: &App) -> usize {
    match &app.active_dialog {
        Some(DialogState::GitLog { selected, .. }) => *selected,
        _ => panic!("not in GitLog dialog"),
    }
}

fn current_dispatch_card_selected(app: &App) -> usize {
    match &app.active_dialog {
        Some(DialogState::DispatchCardMove { selected, .. }) => *selected,
        _ => panic!("not in DispatchCardMove dialog"),
    }
}

fn open_dashboard(app: &mut App) {
    app.mode = AppMode::Dashboard;
    app.active_dialog = Some(DialogState::Dashboard {
        selected: 0,
        scroll_offset: 0,
    });
}

fn open_logs(app: &mut App) {
    app.mode = AppMode::Logs;
    app.active_dialog = Some(DialogState::Logs {
        scroll: u16::MAX,
        level_filter: 0,
        selected: usize::MAX,
        hscroll: 0,
    });
}

fn current_logs_state(app: &App) -> (usize, u8) {
    match &app.active_dialog {
        Some(DialogState::Logs {
            selected,
            level_filter,
            ..
        }) => (*selected, *level_filter),
        _ => panic!("not in Logs dialog"),
    }
}

fn push_log_entry(app: &App, level: tracing::Level, message: &str) {
    app.log_buffer.lock().push_back(LogEntry {
        timestamp: "00:00:00".to_string(),
        level,
        target: "test".to_string(),
        message: message.to_string(),
    });
}

fn open_git_stash(app: &mut App, entries: Vec<(String, String)>) {
    app.mode = AppMode::GitStash;
    app.active_dialog = Some(DialogState::GitStash {
        entries,
        selected: 0,
        scroll: 0,
        input_mode: false,
        input_buffer: String::new(),
        input_cursor: 0,
    });
}

fn git_stash_input_mode(app: &App) -> bool {
    match &app.active_dialog {
        Some(DialogState::GitStash { input_mode, .. }) => *input_mode,
        _ => panic!("not in GitStash dialog"),
    }
}

fn git_stash_input_buffer(app: &App) -> String {
    match &app.active_dialog {
        Some(DialogState::GitStash { input_buffer, .. }) => input_buffer.clone(),
        _ => panic!("not in GitStash dialog"),
    }
}

fn open_import_agents(app: &mut App, discovered: Vec<(String, String, String, bool)>) {
    let n = discovered.len();
    app.mode = AppMode::ImportAgents;
    app.active_dialog = Some(DialogState::ImportAgents {
        discovered,
        selected: vec![false; n],
        cursor: 0,
    });
}

fn import_agents_state(app: &App) -> (Vec<bool>, usize) {
    match &app.active_dialog {
        Some(DialogState::ImportAgents {
            selected, cursor, ..
        }) => (selected.clone(), *cursor),
        _ => panic!("not in ImportAgents dialog"),
    }
}

fn make_agent(id: i64, name: &str) -> AgentProfile {
    AgentProfile {
        id: Some(id),
        source_repo: "/tmp".to_string(),
        name: name.to_string(),
        provider: "claude".to_string(),
        role: "test role".to_string(),
        version: 1,
        last_synced_at: None,
    }
}

fn open_manage_agents(app: &mut App, selected: usize) {
    app.mode = AppMode::ManageAgents;
    app.active_dialog = Some(DialogState::ManageAgents { selected });
}

fn manage_agents_selected(app: &App) -> usize {
    match &app.active_dialog {
        Some(DialogState::ManageAgents { selected }) => *selected,
        _ => panic!("not in ManageAgents dialog"),
    }
}

fn open_manage_providers(app: &mut App, selected: usize) {
    app.mode = AppMode::ManageProviders;
    app.active_dialog = Some(DialogState::ManageProviders { selected });
}

fn open_edit_provider(app: &mut App) {
    app.mode = AppMode::EditProvider;
    app.active_dialog = Some(DialogState::EditProvider {
        original_name: None,
        name: String::new(),
        name_cursor: 0,
        description: String::new(),
        desc_cursor: 0,
        command: String::new(),
        command_cursor: 0,
        default_args: String::new(),
        args_cursor: 0,
        prompt_format_idx: 0,
        prompt_flag: String::new(),
        flag_cursor: 0,
        dispatchable: true,
        agent_dir: String::new(),
        agent_dir_cursor: 0,
        active_field: EditProviderField::Name,
    });
}

fn edit_provider_active_field(app: &App) -> EditProviderField {
    match &app.active_dialog {
        Some(DialogState::EditProvider { active_field, .. }) => *active_field,
        _ => panic!("not in EditProvider dialog"),
    }
}

fn edit_provider_field_state(app: &App) -> (String, usize, usize, bool) {
    // (name, prompt_format_idx, name_cursor, dispatchable) — the bits exercised
    // by the tests below.
    match &app.active_dialog {
        Some(DialogState::EditProvider {
            name,
            name_cursor,
            prompt_format_idx,
            dispatchable,
            ..
        }) => (name.clone(), *prompt_format_idx, *name_cursor, *dispatchable),
        _ => panic!("not in EditProvider dialog"),
    }
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

// ── ConfirmCloseTab ──────────────────────────────────────────────────────

#[test]
fn confirm_close_tab_yes_dismisses_without_panicking_on_empty_workspaces() {
    let mut app = test_app();
    assert!(app.workspaces.is_empty());
    open_confirm_close_tab(&mut app, 0);

    let action = handle_confirm_close_tab_input(&mut app, key(KeyCode::Char('y')));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_close_tab_no_dismisses() {
    let mut app = test_app();
    open_confirm_close_tab(&mut app, 2);

    let action = handle_confirm_close_tab_input(&mut app, key(KeyCode::Char('n')));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_close_tab_esc_dismisses() {
    let mut app = test_app();
    open_confirm_close_tab(&mut app, 0);

    let action = handle_confirm_close_tab_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_close_tab_irrelevant_key_keeps_dialog_open() {
    let mut app = test_app();
    open_confirm_close_tab(&mut app, 0);

    let action = handle_confirm_close_tab_input(&mut app, key(KeyCode::Char('a')));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ConfirmCloseTab { target: 0 })
    ));
    assert_eq!(app.mode, AppMode::ConfirmCloseTab);
}

#[test]
fn confirm_close_tab_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_confirm_close_tab_input(&mut app, key(KeyCode::Char('y')));
    assert!(action.is_none());
}

// ── ConfirmQuit ──────────────────────────────────────────────────────────

#[test]
fn confirm_quit_enter_sets_should_quit_and_dismisses() {
    let mut app = test_app();
    assert!(!app.should_quit);
    open_confirm_quit(&mut app);

    let action = handle_confirm_quit_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert!(app.should_quit);
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_quit_yes_sets_should_quit_and_dismisses() {
    let mut app = test_app();
    open_confirm_quit(&mut app);

    let action = handle_confirm_quit_input(&mut app, key(KeyCode::Char('y')));

    assert!(action.is_none());
    assert!(app.should_quit);
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_quit_no_dismisses_without_quitting() {
    let mut app = test_app();
    open_confirm_quit(&mut app);

    let action = handle_confirm_quit_input(&mut app, key(KeyCode::Char('n')));

    assert!(action.is_none());
    assert!(!app.should_quit);
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_quit_esc_dismisses_without_quitting() {
    let mut app = test_app();
    open_confirm_quit(&mut app);

    let action = handle_confirm_quit_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(!app.should_quit);
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_quit_irrelevant_key_keeps_dialog_open() {
    let mut app = test_app();
    open_confirm_quit(&mut app);

    let action = handle_confirm_quit_input(&mut app, key(KeyCode::Char('a')));

    assert!(action.is_none());
    assert!(!app.should_quit);
    assert!(matches!(app.active_dialog, Some(DialogState::ConfirmQuit)));
    assert_eq!(app.mode, AppMode::ConfirmQuit);
}

// ── ConfirmMerge ─────────────────────────────────────────────────────────

#[test]
fn confirm_merge_m_emits_merge_strategy_and_dismisses() {
    let mut app = test_app();
    open_confirm_merge(&mut app);

    let action = handle_confirm_merge_input(&mut app, key(KeyCode::Char('m')));

    assert!(matches!(action, Some(Action::GitMerge(MergeStrategy::Merge))));
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_merge_r_emits_rebase_strategy_and_dismisses() {
    let mut app = test_app();
    open_confirm_merge(&mut app);

    let action = handle_confirm_merge_input(&mut app, key(KeyCode::Char('r')));

    assert!(matches!(
        action,
        Some(Action::GitMerge(MergeStrategy::Rebase))
    ));
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_merge_esc_dismisses_without_action() {
    let mut app = test_app();
    open_confirm_merge(&mut app);

    let action = handle_confirm_merge_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_merge_irrelevant_key_keeps_dialog_open() {
    let mut app = test_app();
    open_confirm_merge(&mut app);

    let action = handle_confirm_merge_input(&mut app, key(KeyCode::Char('x')));

    assert!(action.is_none());
    assert!(matches!(app.active_dialog, Some(DialogState::ConfirmMerge)));
    assert_eq!(app.mode, AppMode::ConfirmMerge);
}

// ── Help (scroll dialog) ────────────────────────────────────────────────

#[test]
fn help_j_increments_scroll() {
    let mut app = test_app();
    open_help(&mut app, 0);

    let action = handle_help_input(&mut app, key(KeyCode::Char('j')));

    assert!(action.is_none());
    assert_eq!(current_help_scroll(&app), 1);
}

#[test]
fn help_down_arrow_increments_scroll() {
    let mut app = test_app();
    open_help(&mut app, 3);

    handle_help_input(&mut app, key(KeyCode::Down));

    assert_eq!(current_help_scroll(&app), 4);
}

#[test]
fn help_k_decrements_scroll_with_saturating_floor() {
    let mut app = test_app();
    open_help(&mut app, 0);

    handle_help_input(&mut app, key(KeyCode::Char('k')));

    assert_eq!(current_help_scroll(&app), 0);
}

#[test]
fn help_page_down_jumps_ten_lines() {
    let mut app = test_app();
    open_help(&mut app, 5);

    let action = handle_help_input(
        &mut app,
        key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL),
    );

    assert!(action.is_none());
    assert_eq!(current_help_scroll(&app), 15);
}

#[test]
fn help_page_up_saturates_at_zero() {
    let mut app = test_app();
    open_help(&mut app, 3);

    handle_help_input(
        &mut app,
        key_with_mods(KeyCode::Char('u'), KeyModifiers::CONTROL),
    );

    assert_eq!(current_help_scroll(&app), 0);
}

#[test]
fn help_scroll_top_resets_to_zero() {
    let mut app = test_app();
    open_help(&mut app, 100);

    handle_help_input(&mut app, key(KeyCode::Char('g')));

    assert_eq!(current_help_scroll(&app), 0);
}

#[test]
fn help_scroll_bottom_jumps_to_max() {
    let mut app = test_app();
    open_help(&mut app, 0);

    handle_help_input(
        &mut app,
        key_with_mods(KeyCode::Char('G'), KeyModifiers::SHIFT),
    );

    assert_eq!(current_help_scroll(&app), u16::MAX);
}

#[test]
fn help_esc_dismisses() {
    let mut app = test_app();
    open_help(&mut app, 10);

    let action = handle_help_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_q_also_dismisses() {
    let mut app = test_app();
    open_help(&mut app, 0);

    handle_help_input(&mut app, key(KeyCode::Char('q')));

    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_help_input(&mut app, key(KeyCode::Char('j')));
    assert!(action.is_none());
}

// ── About (dismiss-only dialog) ────────────────────────────────────────

#[test]
fn about_esc_dismisses() {
    let mut app = test_app();
    open_about(&mut app);

    let action = handle_about_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn about_irrelevant_key_keeps_dialog_open() {
    let mut app = test_app();
    open_about(&mut app);

    let action = handle_about_input(&mut app, key(KeyCode::Char('x')));

    assert!(action.is_none());
    assert!(matches!(app.active_dialog, Some(DialogState::About)));
    assert_eq!(app.mode, AppMode::About);
}

// ── WorkspaceInfo (horizontal scroll dialog) ───────────────────────────

#[test]
fn workspace_info_l_increments_hscroll_by_four() {
    let mut app = test_app();
    open_workspace_info(&mut app, 0);

    let action = handle_workspace_info_input(&mut app, key(KeyCode::Char('l')));

    assert!(action.is_none());
    assert_eq!(current_workspace_info_hscroll(&app), 4);
}

#[test]
fn workspace_info_right_arrow_increments_hscroll() {
    let mut app = test_app();
    open_workspace_info(&mut app, 8);

    handle_workspace_info_input(&mut app, key(KeyCode::Right));

    assert_eq!(current_workspace_info_hscroll(&app), 12);
}

#[test]
fn workspace_info_h_decrements_hscroll_with_saturating_floor() {
    let mut app = test_app();
    open_workspace_info(&mut app, 2);

    handle_workspace_info_input(&mut app, key(KeyCode::Char('h')));

    // 2 - 4 saturates to 0
    assert_eq!(current_workspace_info_hscroll(&app), 0);
}

#[test]
fn workspace_info_esc_dismisses() {
    let mut app = test_app();
    open_workspace_info(&mut app, 12);

    let action = handle_workspace_info_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn workspace_info_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_workspace_info_input(&mut app, key(KeyCode::Char('l')));
    assert!(action.is_none());
}

// ── DispatchCardMove ───────────────────────────────────────────────────

fn sample_columns() -> Vec<(String, String)> {
    vec![
        ("todo".to_string(), "To Do".to_string()),
        ("doing".to_string(), "Doing".to_string()),
        ("done".to_string(), "Done".to_string()),
    ]
}

#[test]
fn dispatch_card_move_j_advances_selection() {
    let mut app = test_app();
    open_dispatch_card_move(&mut app, 0, sample_columns());

    handle_dispatch_card_move_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(current_dispatch_card_selected(&app), 1);
}

#[test]
fn dispatch_card_move_k_decrements_with_floor_clamp() {
    let mut app = test_app();
    open_dispatch_card_move(&mut app, 0, sample_columns());

    handle_dispatch_card_move_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(current_dispatch_card_selected(&app), 0);
}

#[test]
fn dispatch_card_move_does_not_wrap() {
    let mut app = test_app();
    open_dispatch_card_move(&mut app, 0, sample_columns());

    // Walk to end then attempt to advance — should clamp at last
    for _ in 0..10 {
        handle_dispatch_card_move_input(&mut app, key(KeyCode::Down));
    }
    assert_eq!(current_dispatch_card_selected(&app), 2);
}

#[test]
fn dispatch_card_move_enter_emits_delete_with_selected_column() {
    let mut app = test_app();
    open_dispatch_card_move(&mut app, 5, sample_columns());
    handle_dispatch_card_move_input(&mut app, key(KeyCode::Char('j')));
    handle_dispatch_card_move_input(&mut app, key(KeyCode::Char('j')));

    let action = handle_dispatch_card_move_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::DeleteWorkspace(idx, Some(col_id))) => {
            assert_eq!(idx, 5);
            assert_eq!(col_id, "done");
        }
        _ => panic!("expected DeleteWorkspace with column"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.active_pane, ActivePane::WorkspaceList);
}

#[test]
fn dispatch_card_move_esc_dismisses_without_action() {
    let mut app = test_app();
    open_dispatch_card_move(&mut app, 0, sample_columns());

    let action = handle_dispatch_card_move_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn dispatch_card_move_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_dispatch_card_move_input(&mut app, key(KeyCode::Enter));
    assert!(action.is_none());
}

// ── GitLog ──────────────────────────────────────────────────────────────

fn sample_git_log() -> Vec<GitLogEntry> {
    vec![
        GitLogEntry {
            raw_line: "* abcdef1 first".to_string(),
            sha: Some("abcdef1".to_string()),
        },
        GitLogEntry {
            raw_line: "* abcdef2 second".to_string(),
            sha: Some("abcdef2".to_string()),
        },
        GitLogEntry {
            raw_line: "| merge marker".to_string(),
            sha: None,
        },
    ]
}

#[test]
fn git_log_j_advances_clamped() {
    let mut app = test_app();
    open_git_log(&mut app, sample_git_log());

    handle_git_log_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(current_git_log_selected(&app), 1);
    // Step past the end stays clamped
    for _ in 0..10 {
        handle_git_log_input(&mut app, key(KeyCode::Char('j')));
    }
    assert_eq!(current_git_log_selected(&app), 2);
}

#[test]
fn git_log_k_decrements_with_floor() {
    let mut app = test_app();
    open_git_log(&mut app, sample_git_log());
    handle_git_log_input(&mut app, key(KeyCode::Char('j')));

    handle_git_log_input(&mut app, key(KeyCode::Char('k')));
    handle_git_log_input(&mut app, key(KeyCode::Char('k')));

    assert_eq!(current_git_log_selected(&app), 0);
}

#[test]
fn git_log_select_with_sha_emits_view_commit_diff() {
    let mut app = test_app();
    open_git_log(&mut app, sample_git_log());
    handle_git_log_input(&mut app, key(KeyCode::Char('j')));

    let action = handle_git_log_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::ViewCommitDiff(sha)) => assert_eq!(sha, "abcdef2"),
        _ => panic!("expected ViewCommitDiff"),
    }
}

#[test]
fn git_log_select_without_sha_emits_no_action() {
    let mut app = test_app();
    open_git_log(&mut app, sample_git_log());
    // Move to the entry with sha=None
    handle_git_log_input(&mut app, key(KeyCode::Char('j')));
    handle_git_log_input(&mut app, key(KeyCode::Char('j')));

    let action = handle_git_log_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    // Dialog stays open
    assert!(matches!(app.active_dialog, Some(DialogState::GitLog { .. })));
}

#[test]
fn git_log_esc_dismisses() {
    let mut app = test_app();
    open_git_log(&mut app, sample_git_log());

    let action = handle_git_log_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn git_log_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_git_log_input(&mut app, key(KeyCode::Char('j')));
    assert!(action.is_none());
}

// ── Dashboard ──────────────────────────────────────────────────────────

#[test]
fn dashboard_with_empty_workspaces_auto_dismisses() {
    // Per handler: any keypress with `workspaces.is_empty()` clears the
    // dialog and returns to Normal. Verifying it doesn't panic and exits.
    let mut app = test_app();
    assert!(app.workspaces.is_empty());
    open_dashboard(&mut app);

    let action = handle_dashboard_input(&mut app, key(KeyCode::Char('j')));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn dashboard_esc_dismisses() {
    let mut app = test_app();
    open_dashboard(&mut app);

    let action = handle_dashboard_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn dashboard_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_dashboard_input(&mut app, key(KeyCode::Char('j')));
    assert!(action.is_none());
}

// ── Logs ───────────────────────────────────────────────────────────────

#[test]
fn logs_filter_digit_sets_level_and_resets_selection() {
    let mut app = test_app();
    push_log_entry(&app, tracing::Level::INFO, "hello");
    push_log_entry(&app, tracing::Level::ERROR, "bad");
    open_logs(&mut app);

    handle_logs_input(&mut app, key(KeyCode::Char('1')));

    let (selected, filter) = current_logs_state(&app);
    assert_eq!(filter, 1);
    // sentinel resolved after a Down press to a concrete value
    handle_logs_input(&mut app, key(KeyCode::Char('j')));
    let (selected_after, _) = current_logs_state(&app);
    let _ = selected;
    assert_ne!(selected_after, usize::MAX);
}

#[test]
fn logs_navigation_with_empty_buffer_does_not_panic() {
    let mut app = test_app();
    open_logs(&mut app);

    let a = handle_logs_input(&mut app, key(KeyCode::Char('j')));
    let b = handle_logs_input(&mut app, key(KeyCode::Char('k')));
    let c = handle_logs_input(
        &mut app,
        key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL),
    );

    assert!(a.is_none() && b.is_none() && c.is_none());
}

#[test]
fn logs_copy_with_empty_buffer_does_not_panic() {
    let mut app = test_app();
    open_logs(&mut app);

    let action = handle_logs_input(
        &mut app,
        key_with_mods(KeyCode::Char('y'), KeyModifiers::NONE),
    );
    // copy bound to 'y' by default; whether it's recognized or not, no panic
    assert!(action.is_none());
}

#[test]
fn logs_esc_dismisses() {
    let mut app = test_app();
    open_logs(&mut app);

    let action = handle_logs_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn logs_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_logs_input(&mut app, key(KeyCode::Char('j')));
    assert!(action.is_none());
}

// ── GitStash ───────────────────────────────────────────────────────────

fn sample_stash_entries() -> Vec<(String, String)> {
    vec![
        ("stash@{0}".to_string(), "WIP feature".to_string()),
        ("stash@{1}".to_string(), "bugfix".to_string()),
    ]
}

#[test]
fn git_stash_s_enters_input_mode() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());
    assert!(!git_stash_input_mode(&app));

    handle_git_stash_input(&mut app, key(KeyCode::Char('s')));

    assert!(git_stash_input_mode(&app));
}

#[test]
fn git_stash_input_mode_inserts_chars_and_backspace_removes() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());
    handle_git_stash_input(&mut app, key(KeyCode::Char('s'))); // enter input mode

    for c in "wip".chars() {
        handle_git_stash_input(&mut app, key(KeyCode::Char(c)));
    }
    assert_eq!(git_stash_input_buffer(&app), "wip");

    handle_git_stash_input(&mut app, key(KeyCode::Backspace));
    assert_eq!(git_stash_input_buffer(&app), "wi");
}

#[test]
fn git_stash_input_mode_esc_exits_to_list_mode() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());
    handle_git_stash_input(&mut app, key(KeyCode::Char('s')));
    assert!(git_stash_input_mode(&app));

    handle_git_stash_input(&mut app, key(KeyCode::Esc));

    // Dialog still open, but back in list mode
    assert!(matches!(app.active_dialog, Some(DialogState::GitStash { .. })));
    assert!(!git_stash_input_mode(&app));
}

#[test]
fn git_stash_input_mode_enter_emits_save_with_buffer() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());
    handle_git_stash_input(&mut app, key(KeyCode::Char('s')));
    for c in "msg".chars() {
        handle_git_stash_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_git_stash_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::GitStashSave(msg)) => assert_eq!(msg, "msg"),
        _ => panic!("expected GitStashSave"),
    }
}

#[test]
fn git_stash_list_p_emits_pop_for_selected_index() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());
    handle_git_stash_input(&mut app, key(KeyCode::Char('j')));

    let action = handle_git_stash_input(&mut app, key(KeyCode::Char('p')));

    assert!(matches!(action, Some(Action::GitStashPop(1))));
}

#[test]
fn git_stash_list_a_emits_apply() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());

    let action = handle_git_stash_input(&mut app, key(KeyCode::Char('a')));

    assert!(matches!(action, Some(Action::GitStashApply(0))));
}

#[test]
fn git_stash_list_d_emits_drop() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());

    let action = handle_git_stash_input(&mut app, key(KeyCode::Char('d')));

    assert!(matches!(action, Some(Action::GitStashDrop(0))));
}

#[test]
fn git_stash_list_enter_emits_show() {
    // "show" is bound to Enter by default (not 'w' as initially planned).
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());

    let action = handle_git_stash_input(&mut app, key(KeyCode::Enter));

    assert!(matches!(action, Some(Action::GitStashShow(0))));
}

#[test]
fn git_stash_list_esc_dismisses() {
    let mut app = test_app();
    open_git_stash(&mut app, sample_stash_entries());

    let action = handle_git_stash_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

// ── ImportAgents ───────────────────────────────────────────────────────

fn sample_imports() -> Vec<(String, String, String, bool)> {
    vec![
        ("alice".to_string(), "claude".to_string(), "role a".to_string(), false),
        ("bob".to_string(), "claude".to_string(), "role b".to_string(), false),
        ("carol".to_string(), "claude".to_string(), "role c".to_string(), true),
    ]
}

#[test]
fn import_agents_j_wraps_cursor() {
    let mut app = test_app();
    open_import_agents(&mut app, sample_imports());

    for _ in 0..3 {
        handle_import_agents_input(&mut app, key(KeyCode::Char('j')));
    }
    let (_, cursor) = import_agents_state(&app);
    assert_eq!(cursor, 0); // wrapped
}

#[test]
fn import_agents_space_toggles_and_advances() {
    let mut app = test_app();
    open_import_agents(&mut app, sample_imports());

    handle_import_agents_input(&mut app, key(KeyCode::Char(' ')));

    let (selected, cursor) = import_agents_state(&app);
    assert!(selected[0]);
    assert!(!selected[1]);
    assert_eq!(cursor, 1);
}

#[test]
fn import_agents_a_toggles_all() {
    let mut app = test_app();
    open_import_agents(&mut app, sample_imports());

    handle_import_agents_input(&mut app, key(KeyCode::Char('a')));
    let (selected, _) = import_agents_state(&app);
    assert!(selected.iter().all(|&s| s));

    handle_import_agents_input(&mut app, key(KeyCode::Char('a')));
    let (selected, _) = import_agents_state(&app);
    assert!(selected.iter().all(|&s| !s));
}

#[test]
fn import_agents_enter_with_no_selections_returns_to_manage() {
    let mut app = test_app();
    open_import_agents(&mut app, sample_imports());

    let action = handle_import_agents_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::ManageAgents);
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ManageAgents { .. })
    ));
}

#[test]
fn import_agents_enter_with_selections_emits_import_action() {
    let mut app = test_app();
    open_import_agents(&mut app, sample_imports());
    // Select first two
    handle_import_agents_input(&mut app, key(KeyCode::Char(' ')));
    handle_import_agents_input(&mut app, key(KeyCode::Char(' ')));

    let action = handle_import_agents_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::ImportAgents(items)) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].0, "alice");
            assert_eq!(items[1].0, "bob");
        }
        _ => panic!("expected ImportAgents action"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn import_agents_esc_returns_to_manage_agents() {
    let mut app = test_app();
    open_import_agents(&mut app, sample_imports());

    let action = handle_import_agents_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::ManageAgents);
}

#[test]
fn import_agents_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_import_agents_input(&mut app, key(KeyCode::Char('j')));
    assert!(action.is_none());
}

// ── ManageAgents ───────────────────────────────────────────────────────

#[test]
fn manage_agents_j_wraps_selection() {
    let mut app = test_app();
    app.agent_profiles = vec![make_agent(1, "a"), make_agent(2, "b"), make_agent(3, "c")];
    open_manage_agents(&mut app, 0);

    for _ in 0..3 {
        handle_manage_agents_input(&mut app, key(KeyCode::Char('j')));
    }
    assert_eq!(manage_agents_selected(&app), 0); // wrapped
}

#[test]
fn manage_agents_k_wraps_backwards_from_zero() {
    let mut app = test_app();
    app.agent_profiles = vec![make_agent(1, "a"), make_agent(2, "b")];
    open_manage_agents(&mut app, 0);

    handle_manage_agents_input(&mut app, key(KeyCode::Char('k')));

    assert_eq!(manage_agents_selected(&app), 1);
}

#[test]
fn manage_agents_n_opens_new_edit_agent_dialog() {
    let mut app = test_app();
    open_manage_agents(&mut app, 0);

    let action = handle_manage_agents_input(&mut app, key(KeyCode::Char('n')));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::EditAgent);
    match &app.active_dialog {
        Some(DialogState::EditAgent { editing_id, .. }) => assert!(editing_id.is_none()),
        _ => panic!("expected EditAgent dialog"),
    }
}

#[test]
fn manage_agents_d_emits_delete_action() {
    let mut app = test_app();
    app.agent_profiles = vec![make_agent(7, "a")];
    open_manage_agents(&mut app, 0);

    let action = handle_manage_agents_input(&mut app, key(KeyCode::Char('d')));

    assert!(matches!(action, Some(Action::DeleteAgent(7))));
}

#[test]
fn manage_agents_p_emits_sync_to_repo() {
    let mut app = test_app();
    app.agent_profiles = vec![make_agent(42, "a")];
    open_manage_agents(&mut app, 0);

    let action = handle_manage_agents_input(&mut app, key(KeyCode::Char('p')));

    assert!(matches!(action, Some(Action::SyncAgentToRepo(42))));
}

#[test]
fn manage_agents_i_emits_scan_repo_agents() {
    let mut app = test_app();
    open_manage_agents(&mut app, 0);

    let action = handle_manage_agents_input(&mut app, key(KeyCode::Char('i')));

    assert!(matches!(action, Some(Action::ScanRepoAgents)));
}

#[test]
fn manage_agents_esc_dismisses() {
    let mut app = test_app();
    open_manage_agents(&mut app, 0);

    let action = handle_manage_agents_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

// ── ManageProviders ────────────────────────────────────────────────────

#[test]
fn manage_providers_j_no_panic_with_or_without_providers() {
    // Default provider_manager state is whatever load_or_init returns at
    // test_app() construction; we just verify the handler is safe.
    let mut app = test_app();
    open_manage_providers(&mut app, 0);

    let action = handle_manage_providers_input(&mut app, key(KeyCode::Char('j')));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ManageProviders { .. })
    ));
}

#[test]
fn manage_providers_n_opens_new_edit_provider_dialog() {
    let mut app = test_app();
    open_manage_providers(&mut app, 0);

    let action = handle_manage_providers_input(&mut app, key(KeyCode::Char('n')));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::EditProvider);
    match &app.active_dialog {
        Some(DialogState::EditProvider { original_name, .. }) => {
            assert!(original_name.is_none());
        }
        _ => panic!("expected EditProvider dialog"),
    }
}

#[test]
fn manage_providers_esc_dismisses() {
    let mut app = test_app();
    open_manage_providers(&mut app, 0);

    let action = handle_manage_providers_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn manage_providers_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_manage_providers_input(&mut app, key(KeyCode::Char('j')));
    assert!(action.is_none());
}

// ── EditProvider ───────────────────────────────────────────────────────

#[test]
fn edit_provider_tab_cycles_eight_fields_forward() {
    let mut app = test_app();
    open_edit_provider(&mut app);

    let sequence = [
        EditProviderField::Description,
        EditProviderField::Command,
        EditProviderField::DefaultArgs,
        EditProviderField::PromptFormat,
        EditProviderField::PromptFlag,
        EditProviderField::Dispatchable,
        EditProviderField::AgentDir,
        EditProviderField::Name, // wrap
    ];
    for expected in sequence {
        handle_edit_provider_input(&mut app, key(KeyCode::Tab));
        assert_eq!(edit_provider_active_field(&app), expected);
    }
}

#[test]
fn edit_provider_back_tab_cycles_in_reverse() {
    let mut app = test_app();
    open_edit_provider(&mut app);

    handle_edit_provider_input(&mut app, key(KeyCode::BackTab));
    assert_eq!(edit_provider_active_field(&app), EditProviderField::AgentDir);

    handle_edit_provider_input(&mut app, key(KeyCode::BackTab));
    assert_eq!(
        edit_provider_active_field(&app),
        EditProviderField::Dispatchable
    );
}

#[test]
fn edit_provider_text_input_writes_to_active_field() {
    let mut app = test_app();
    open_edit_provider(&mut app);

    for c in "claude".chars() {
        handle_edit_provider_input(&mut app, key(KeyCode::Char(c)));
    }

    let (name, _, cursor, _) = edit_provider_field_state(&app);
    assert_eq!(name, "claude");
    assert_eq!(cursor, 6);
}

#[test]
fn edit_provider_backspace_deletes_previous_char() {
    let mut app = test_app();
    open_edit_provider(&mut app);
    for c in "abc".chars() {
        handle_edit_provider_input(&mut app, key(KeyCode::Char(c)));
    }

    handle_edit_provider_input(&mut app, key(KeyCode::Backspace));

    let (name, _, cursor, _) = edit_provider_field_state(&app);
    assert_eq!(name, "ab");
    assert_eq!(cursor, 2);
}

#[test]
fn edit_provider_prompt_format_right_cycles_through_three_values() {
    let mut app = test_app();
    open_edit_provider(&mut app);
    // Move to PromptFormat (Tab x4: Name → Desc → Cmd → Args → PromptFormat)
    for _ in 0..4 {
        handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    }
    assert_eq!(
        edit_provider_active_field(&app),
        EditProviderField::PromptFormat
    );

    handle_edit_provider_input(&mut app, key(KeyCode::Right));
    assert_eq!(edit_provider_field_state(&app).1, 1);

    handle_edit_provider_input(&mut app, key(KeyCode::Right));
    assert_eq!(edit_provider_field_state(&app).1, 2);

    handle_edit_provider_input(&mut app, key(KeyCode::Right));
    assert_eq!(edit_provider_field_state(&app).1, 0); // wrap
}

#[test]
fn edit_provider_prompt_format_left_cycles_backwards() {
    let mut app = test_app();
    open_edit_provider(&mut app);
    for _ in 0..4 {
        handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    }

    handle_edit_provider_input(&mut app, key(KeyCode::Left));
    assert_eq!(edit_provider_field_state(&app).1, 2); // wraps to 2
}

#[test]
fn edit_provider_dispatchable_space_toggles() {
    let mut app = test_app();
    open_edit_provider(&mut app);
    // Move to Dispatchable (Tab x6: Name → ... → Dispatchable)
    for _ in 0..6 {
        handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    }
    assert_eq!(
        edit_provider_active_field(&app),
        EditProviderField::Dispatchable
    );
    let (_, _, _, initial) = edit_provider_field_state(&app);

    handle_edit_provider_input(&mut app, key(KeyCode::Char(' ')));

    let (_, _, _, after) = edit_provider_field_state(&app);
    assert_eq!(after, !initial);
}

#[test]
fn edit_provider_dispatchable_arrow_keys_also_toggle() {
    let mut app = test_app();
    open_edit_provider(&mut app);
    for _ in 0..6 {
        handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    }
    let (_, _, _, initial) = edit_provider_field_state(&app);

    handle_edit_provider_input(&mut app, key(KeyCode::Left));
    let (_, _, _, after_left) = edit_provider_field_state(&app);
    assert_eq!(after_left, !initial);

    handle_edit_provider_input(&mut app, key(KeyCode::Right));
    let (_, _, _, after_right) = edit_provider_field_state(&app);
    assert_eq!(after_right, initial);
}

#[test]
fn edit_provider_esc_returns_to_manage_providers_not_normal() {
    let mut app = test_app();
    open_edit_provider(&mut app);

    let action = handle_edit_provider_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    // Esc transitions back to manager, NOT Normal — distinct from most dialogs.
    assert_eq!(app.mode, AppMode::ManageProviders);
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ManageProviders { .. })
    ));
}

#[test]
fn edit_provider_ctrl_s_with_empty_name_shows_error_toast_and_does_not_save() {
    let mut app = test_app();
    let before_count = app.provider_manager.all().len();
    open_edit_provider(&mut app);
    // Fill command but leave name empty
    for _ in 0..2 {
        handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    }
    for c in "cmd".chars() {
        handle_edit_provider_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_edit_provider_input(
        &mut app,
        key_with_mods(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    assert!(action.is_none());
    assert!(app.toast.is_some());
    assert_eq!(app.provider_manager.all().len(), before_count);
    // Dialog stays open
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::EditProvider { .. })
    ));
}

#[test]
fn edit_provider_ctrl_s_with_empty_command_shows_error_toast() {
    let mut app = test_app();
    let before_count = app.provider_manager.all().len();
    open_edit_provider(&mut app);
    // Fill name but leave command empty
    for c in "my-provider".chars() {
        handle_edit_provider_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_edit_provider_input(
        &mut app,
        key_with_mods(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    assert!(action.is_none());
    assert!(app.toast.is_some());
    assert_eq!(app.provider_manager.all().len(), before_count);
}

#[test]
fn edit_provider_ctrl_s_with_valid_data_saves_and_returns_to_manager() {
    // Uses isolated paths because Ctrl+S persists providers.toml to disk.
    // _tmp keeps the temp dir alive until the test ends.
    let (mut app, _tmp) = test_app_isolated();
    open_edit_provider(&mut app);
    // Fill name
    for c in "test-pilot".chars() {
        handle_edit_provider_input(&mut app, key(KeyCode::Char(c)));
    }
    // Tab to Command (Name → Desc → Cmd)
    handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    for c in "/usr/bin/echo".chars() {
        handle_edit_provider_input(&mut app, key(KeyCode::Char(c)));
    }

    let action = handle_edit_provider_input(
        &mut app,
        key_with_mods(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::ManageProviders);
    // Provider got persisted into the manager
    assert!(
        app.provider_manager
            .all()
            .iter()
            .any(|p| p.name == "test-pilot")
    );
}

#[test]
fn edit_provider_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_edit_provider_input(&mut app, key(KeyCode::Tab));
    assert!(action.is_none());
}

// ── NewTab ────────────────────────────────────────────────────────────────
//
// Hierarchical menu: Main → Agents/Tools. `test_app_isolated()` bootstraps
// providers.toml with the two defaults (Claude Code + Gemini), so the Agents
// submenu has a deterministic 2-entry list from `app.new_tab_agent_list()`.

fn open_new_tab_main(app: &mut App) {
    app.mode = AppMode::NewTab;
    app.active_dialog = Some(DialogState::NewTab {
        menu: NewTabMenu::Main,
    });
}

fn open_new_tab_agents(app: &mut App, selected: usize) {
    app.mode = AppMode::NewTab;
    app.active_dialog = Some(DialogState::NewTab {
        menu: NewTabMenu::Agents { selected },
    });
}

fn open_new_tab_tools(app: &mut App) {
    app.mode = AppMode::NewTab;
    app.active_dialog = Some(DialogState::NewTab {
        menu: NewTabMenu::Tools,
    });
}

fn current_new_tab_menu(app: &App) -> NewTabMenu {
    match app.active_dialog {
        Some(DialogState::NewTab { ref menu }) => menu.clone(),
        _ => panic!("expected NewTab dialog"),
    }
}

#[test]
fn new_tab_main_key_1_spawns_shell() {
    let mut app = test_app();
    open_new_tab_main(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('1')));

    assert!(matches!(action, Some(Action::SpawnTab(AIProvider::Shell))));
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn new_tab_main_key_2_opens_agents_submenu() {
    let mut app = test_app();
    open_new_tab_main(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('2')));

    assert!(action.is_none());
    assert_eq!(
        current_new_tab_menu(&app),
        NewTabMenu::Agents { selected: 0 }
    );
}

#[test]
fn new_tab_main_key_3_opens_tools_submenu() {
    let mut app = test_app();
    open_new_tab_main(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('3')));

    assert!(action.is_none());
    assert_eq!(current_new_tab_menu(&app), NewTabMenu::Tools);
}

#[test]
fn new_tab_main_esc_dismisses() {
    let mut app = test_app();
    open_new_tab_main(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn new_tab_main_unknown_key_is_noop() {
    let mut app = test_app();
    open_new_tab_main(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('x')));

    assert!(action.is_none());
    assert_eq!(current_new_tab_menu(&app), NewTabMenu::Main);
}

#[test]
fn new_tab_agents_j_advances_selection_with_wrap() {
    let (mut app, _tmp) = test_app_isolated();
    let count = app.new_tab_agent_list().len();
    assert!(count >= 2, "default providers.toml seeds at least 2 entries");
    open_new_tab_agents(&mut app, 0);

    handle_new_tab_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(
        current_new_tab_menu(&app),
        NewTabMenu::Agents { selected: 1 }
    );

    // Wrap to 0 at the end
    open_new_tab_agents(&mut app, count - 1);
    handle_new_tab_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(
        current_new_tab_menu(&app),
        NewTabMenu::Agents { selected: 0 }
    );
}

#[test]
fn new_tab_agents_k_retreats_selection_with_wrap() {
    let (mut app, _tmp) = test_app_isolated();
    let count = app.new_tab_agent_list().len();
    open_new_tab_agents(&mut app, 1);

    handle_new_tab_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(
        current_new_tab_menu(&app),
        NewTabMenu::Agents { selected: 0 }
    );

    // Wrap to last from 0
    handle_new_tab_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(
        current_new_tab_menu(&app),
        NewTabMenu::Agents {
            selected: count - 1
        }
    );
}

#[test]
fn new_tab_agents_enter_spawns_selected_provider() {
    let (mut app, _tmp) = test_app_isolated();
    let providers = app.new_tab_agent_list();
    let expected = providers
        .first()
        .cloned()
        .expect("providers.toml has at least one entry");
    open_new_tab_agents(&mut app, 0);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::SpawnTab(p)) => assert_eq!(p, expected),
        other => panic!("expected SpawnTab({expected:?}), got {other:?}"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn new_tab_agents_digit_shortcut_spawns_indexed_provider() {
    let (mut app, _tmp) = test_app_isolated();
    let providers = app.new_tab_agent_list();
    let second = providers
        .get(1)
        .cloned()
        .expect("at least 2 default providers");
    open_new_tab_agents(&mut app, 0);

    // '2' selects index 1 regardless of current selection
    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('2')));

    match action {
        Some(Action::SpawnTab(p)) => assert_eq!(p, second),
        other => panic!("expected SpawnTab({second:?}), got {other:?}"),
    }
    assert!(app.active_dialog.is_none());
}

#[test]
fn new_tab_agents_digit_out_of_range_is_noop() {
    let (mut app, _tmp) = test_app_isolated();
    let count = app.new_tab_agent_list().len();
    assert!(count < 9, "this test assumes fewer than 9 default providers");
    open_new_tab_agents(&mut app, 0);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('9')));

    assert!(action.is_none());
    // Dialog still open with selection unchanged
    assert_eq!(
        current_new_tab_menu(&app),
        NewTabMenu::Agents { selected: 0 }
    );
}

#[test]
fn new_tab_agents_esc_returns_to_main() {
    let (mut app, _tmp) = test_app_isolated();
    open_new_tab_agents(&mut app, 0);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert_eq!(current_new_tab_menu(&app), NewTabMenu::Main);
}

#[test]
fn new_tab_tools_key_1_spawns_kanban() {
    let mut app = test_app();
    open_new_tab_tools(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('1')));

    assert!(matches!(
        action,
        Some(Action::SpawnTab(AIProvider::Kanban))
    ));
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn new_tab_tools_key_2_spawns_code_review() {
    let mut app = test_app();
    open_new_tab_tools(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('2')));

    assert!(matches!(
        action,
        Some(Action::SpawnTab(AIProvider::CodeReview))
    ));
    assert!(app.active_dialog.is_none());
}

#[test]
fn new_tab_tools_key_3_spawns_api() {
    let mut app = test_app();
    open_new_tab_tools(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('3')));

    assert!(matches!(action, Some(Action::SpawnTab(AIProvider::Api))));
    assert!(app.active_dialog.is_none());
}

#[test]
fn new_tab_tools_esc_returns_to_main() {
    let mut app = test_app();
    open_new_tab_tools(&mut app);

    let action = handle_new_tab_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert_eq!(current_new_tab_menu(&app), NewTabMenu::Main);
}

#[test]
fn new_tab_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_new_tab_input(&mut app, key(KeyCode::Char('1')));
    assert!(action.is_none());
}
