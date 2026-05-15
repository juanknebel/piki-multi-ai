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
    handle_conflict_resolution_input, handle_dashboard_input, handle_dispatch_agent_input,
    handle_dispatch_card_move_input, handle_edit_agent_input, handle_edit_agent_role_input,
    handle_edit_provider_input, handle_edit_workspace_input, handle_git_log_input,
    handle_git_stash_input, handle_help_input, handle_import_agents_input, handle_logs_input,
    handle_manage_agents_input, handle_manage_providers_input, handle_new_tab_input,
    handle_new_workspace_input, handle_workspace_info_input,
};
use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::dialog_state::{
    ConflictFile, ConflictStrategy, DialogState, EditAgentField, EditProviderField,
    EditWorkspaceField, GitLogEntry, NewTabMenu,
};
use piki_core::WorkspaceType;
use crate::log_buffer::LogEntry;
use crate::test_support::{key, key_with_mods, test_app, test_app_isolated};
use piki_core::AIProvider;

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
        search_active: false,
        search_buffer: String::new(),
        search_cursor: 0,
        auto_refresh: true,
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

// ── ConflictResolution ────────────────────────────────────────────────────
//
// List of conflicted files with action keys (o/t/m/e/A/Enter) and j/k
// navigation. Keybindings come from `app.config.matches_conflict_resolution`,
// which falls back to the defaults in `default_conflict_resolution()`:
// down=j, up=k, ours=o, theirs=t, mark_resolved=m, edit=e, abort=A,
// select=enter (view diff), exit=esc, exit_alt=X.

fn sample_conflict_files() -> Vec<ConflictFile> {
    vec![
        ConflictFile {
            path: "src/a.rs".into(),
            status: "Conflicted".into(),
        },
        ConflictFile {
            path: "src/b.rs".into(),
            status: "Conflicted".into(),
        },
        ConflictFile {
            path: "src/c.rs".into(),
            status: "Conflicted".into(),
        },
    ]
}

fn open_conflict_resolution(
    app: &mut App,
    files: Vec<ConflictFile>,
    repo_path: std::path::PathBuf,
    selected: usize,
) {
    app.mode = AppMode::ConflictResolution;
    app.active_dialog = Some(DialogState::ConflictResolution {
        files,
        selected,
        repo_path,
    });
}

fn current_conflict_selected(app: &App) -> usize {
    match app.active_dialog {
        Some(DialogState::ConflictResolution { selected, .. }) => selected,
        _ => panic!("expected ConflictResolution dialog"),
    }
}

#[test]
fn conflict_down_advances_with_clamp() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(current_conflict_selected(&app), 1);
    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(current_conflict_selected(&app), 2);
    // Clamp at last
    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(current_conflict_selected(&app), 2);
}

#[test]
fn conflict_up_retreats_with_clamp() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        2,
    );

    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(current_conflict_selected(&app), 1);
    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(current_conflict_selected(&app), 0);
    // Clamp at 0 (saturating_sub)
    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(current_conflict_selected(&app), 0);
}

#[test]
fn conflict_arrow_keys_navigate_as_alt_bindings() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    handle_conflict_resolution_input(&mut app, key(KeyCode::Down));
    assert_eq!(current_conflict_selected(&app), 1);
    handle_conflict_resolution_input(&mut app, key(KeyCode::Up));
    assert_eq!(current_conflict_selected(&app), 0);
}

#[test]
fn conflict_empty_list_navigation_is_noop() {
    let mut app = test_app();
    open_conflict_resolution(&mut app, vec![], "/tmp/repo".into(), 0);

    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('j')));
    assert_eq!(current_conflict_selected(&app), 0);
    handle_conflict_resolution_input(&mut app, key(KeyCode::Char('k')));
    assert_eq!(current_conflict_selected(&app), 0);
}

#[test]
fn conflict_ours_returns_resolve_action_for_selected() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        1,
    );

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Char('o')));

    match action {
        Some(Action::ResolveConflict {
            file,
            strategy: ConflictStrategy::Ours,
        }) => assert_eq!(file, "src/b.rs"),
        other => panic!("expected ResolveConflict(b.rs, Ours), got {other:?}"),
    }
}

#[test]
fn conflict_theirs_returns_resolve_action_for_selected() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Char('t')));

    match action {
        Some(Action::ResolveConflict {
            file,
            strategy: ConflictStrategy::Theirs,
        }) => assert_eq!(file, "src/a.rs"),
        other => panic!("expected ResolveConflict(a.rs, Theirs), got {other:?}"),
    }
}

#[test]
fn conflict_mark_resolved_returns_resolve_action() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        2,
    );

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Char('m')));

    match action {
        Some(Action::ResolveConflict {
            file,
            strategy: ConflictStrategy::MarkResolved,
        }) => assert_eq!(file, "src/c.rs"),
        other => panic!("expected ResolveConflict(c.rs, MarkResolved), got {other:?}"),
    }
}

#[test]
fn conflict_enter_returns_view_diff_action() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        1,
    );

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::ViewConflictDiff(file)) => assert_eq!(file, "src/b.rs"),
        other => panic!("expected ViewConflictDiff(b.rs), got {other:?}"),
    }
}

#[test]
fn conflict_edit_returns_open_editor_with_full_path() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Char('e')));

    match action {
        Some(Action::OpenEditor(path)) => {
            assert_eq!(path, std::path::PathBuf::from("/tmp/repo/src/a.rs"));
        }
        other => panic!("expected OpenEditor(/tmp/repo/src/a.rs), got {other:?}"),
    }
}

#[test]
fn conflict_abort_returns_abort_merge() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    // Default `abort` binding is "A" — uppercase parses with SHIFT modifier.
    let action = handle_conflict_resolution_input(
        &mut app,
        key_with_mods(KeyCode::Char('A'), KeyModifiers::SHIFT),
    );

    assert!(matches!(action, Some(Action::AbortMerge)));
    // Dialog stays open — caller decides what to do next.
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ConflictResolution { .. })
    ));
}

#[test]
fn conflict_esc_dismisses_and_clears_diff() {
    let mut app = test_app();
    app.diff_content = Some(std::sync::Arc::new(ratatui::text::Text::raw("x")));
    app.diff_file_path = Some("src/a.rs".into());
    app.interacting = true;
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    assert!(!app.interacting);
    assert!(app.diff_content.is_none());
    assert!(app.diff_file_path.is_none());
}

#[test]
fn conflict_exit_alt_capital_x_also_dismisses() {
    let mut app = test_app();
    open_conflict_resolution(
        &mut app,
        sample_conflict_files(),
        "/tmp/repo".into(),
        0,
    );

    // Default `exit_alt` binding is "X" — uppercase parses with SHIFT modifier.
    let action = handle_conflict_resolution_input(
        &mut app,
        key_with_mods(KeyCode::Char('X'), KeyModifiers::SHIFT),
    );

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn conflict_action_on_empty_list_returns_none() {
    let mut app = test_app();
    open_conflict_resolution(&mut app, vec![], "/tmp/repo".into(), 0);

    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Char('o')));

    assert!(action.is_none());
}

#[test]
fn conflict_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_conflict_resolution_input(&mut app, key(KeyCode::Char('o')));
    assert!(action.is_none());
}

// ── EditAgent (step 1: name + provider) ───────────────────────────────────
//
// Two-field dialog with Tab/BackTab cycling between Name and Provider,
// Left/Right cycling provider_idx when on Provider, alphanumeric+`-_` text
// input on Name, Enter advancing to EditAgentRole, and Esc going back to
// ManageAgents. `test_app_isolated()` seeds 2 default providers.

fn open_edit_agent(
    app: &mut App,
    editing_id: Option<i64>,
    name: &str,
    provider_idx: usize,
    role: &str,
    active_field: EditAgentField,
) {
    app.mode = AppMode::EditAgent;
    app.active_dialog = Some(DialogState::EditAgent {
        editing_id,
        name: name.to_string(),
        name_cursor: name.len(),
        provider_idx,
        role: role.to_string(),
        active_field,
    });
}

fn current_edit_agent_field(app: &App) -> EditAgentField {
    match app.active_dialog {
        Some(DialogState::EditAgent { active_field, .. }) => active_field,
        _ => panic!("expected EditAgent dialog"),
    }
}

fn current_edit_agent_provider_idx(app: &App) -> usize {
    match app.active_dialog {
        Some(DialogState::EditAgent { provider_idx, .. }) => provider_idx,
        _ => panic!("expected EditAgent dialog"),
    }
}

fn current_edit_agent_name(app: &App) -> String {
    match app.active_dialog {
        Some(DialogState::EditAgent { ref name, .. }) => name.clone(),
        _ => panic!("expected EditAgent dialog"),
    }
}

#[test]
fn edit_agent_tab_cycles_name_to_provider() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "agent", 0, "role", EditAgentField::Name);

    handle_edit_agent_input(&mut app, key(KeyCode::Tab));
    assert_eq!(current_edit_agent_field(&app), EditAgentField::Provider);

    handle_edit_agent_input(&mut app, key(KeyCode::Tab));
    assert_eq!(current_edit_agent_field(&app), EditAgentField::Name);
}

#[test]
fn edit_agent_backtab_cycles_same_as_tab_two_variants() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "agent", 0, "", EditAgentField::Name);

    handle_edit_agent_input(&mut app, key(KeyCode::BackTab));
    assert_eq!(current_edit_agent_field(&app), EditAgentField::Provider);
}

#[test]
fn edit_agent_right_on_provider_wraps_forward() {
    let (mut app, _tmp) = test_app_isolated();
    let count = app.new_tab_agent_list().len();
    assert!(count >= 2);
    open_edit_agent(&mut app, None, "agent", 0, "", EditAgentField::Provider);

    handle_edit_agent_input(&mut app, key(KeyCode::Right));
    assert_eq!(current_edit_agent_provider_idx(&app), 1);

    // Wrap around at the end
    open_edit_agent(
        &mut app,
        None,
        "agent",
        count - 1,
        "",
        EditAgentField::Provider,
    );
    handle_edit_agent_input(&mut app, key(KeyCode::Right));
    assert_eq!(current_edit_agent_provider_idx(&app), 0);
}

#[test]
fn edit_agent_left_on_provider_wraps_backward() {
    let (mut app, _tmp) = test_app_isolated();
    let count = app.new_tab_agent_list().len();
    open_edit_agent(&mut app, None, "agent", 1, "", EditAgentField::Provider);

    handle_edit_agent_input(&mut app, key(KeyCode::Left));
    assert_eq!(current_edit_agent_provider_idx(&app), 0);

    // Wrap to last from 0
    handle_edit_agent_input(&mut app, key(KeyCode::Left));
    assert_eq!(current_edit_agent_provider_idx(&app), count - 1);
}

#[test]
fn edit_agent_left_right_on_name_do_not_change_provider() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "agent", 0, "", EditAgentField::Name);

    handle_edit_agent_input(&mut app, key(KeyCode::Right));
    assert_eq!(current_edit_agent_provider_idx(&app), 0);

    handle_edit_agent_input(&mut app, key(KeyCode::Left));
    assert_eq!(current_edit_agent_provider_idx(&app), 0);
}

#[test]
fn edit_agent_text_input_on_name_accepts_alphanumeric_and_dash_underscore() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "", 0, "", EditAgentField::Name);

    for c in "code-pilot_42".chars() {
        handle_edit_agent_input(&mut app, key(KeyCode::Char(c)));
    }

    assert_eq!(current_edit_agent_name(&app), "code-pilot_42");
}

#[test]
fn edit_agent_text_input_on_name_rejects_spaces_and_punctuation() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "", 0, "", EditAgentField::Name);

    for c in "a b.c/d!".chars() {
        handle_edit_agent_input(&mut app, key(KeyCode::Char(c)));
    }

    // Only alphanumeric + `-_` allowed
    assert_eq!(current_edit_agent_name(&app), "abcd");
}

#[test]
fn edit_agent_text_input_ignored_when_provider_field_active() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "init", 0, "", EditAgentField::Provider);

    handle_edit_agent_input(&mut app, key(KeyCode::Char('x')));

    assert_eq!(current_edit_agent_name(&app), "init");
}

#[test]
fn edit_agent_enter_with_empty_name_does_not_transition() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "", 0, "role text", EditAgentField::Name);

    let action = handle_edit_agent_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::EditAgent);
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::EditAgent { .. })
    ));
}

#[test]
fn edit_agent_enter_with_whitespace_name_does_not_transition() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "   ", 0, "", EditAgentField::Name);

    let action = handle_edit_agent_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::EditAgent { .. })
    ));
}

#[test]
fn edit_agent_enter_with_valid_name_advances_to_role_editor() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(
        &mut app,
        Some(7),
        "  agent  ",
        1,
        "previous role",
        EditAgentField::Name,
    );

    let action = handle_edit_agent_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::EditAgentRole);
    match app.active_dialog {
        Some(DialogState::EditAgentRole {
            editing_id,
            ref name,
            provider_idx,
            ref role,
            role_cursor,
            scroll,
        }) => {
            assert_eq!(editing_id, Some(7));
            assert_eq!(name, "agent"); // trimmed
            assert_eq!(provider_idx, 1);
            assert_eq!(role, "previous role");
            assert_eq!(role_cursor, "previous role".len());
            assert_eq!(scroll, 0);
        }
        ref other => panic!("expected EditAgentRole, got {other:?}"),
    }
}

#[test]
fn edit_agent_esc_returns_to_manage_agents() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent(&mut app, None, "agent", 0, "", EditAgentField::Name);

    let action = handle_edit_agent_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::ManageAgents);
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::ManageAgents { .. })
    ));
}

#[test]
fn edit_agent_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_edit_agent_input(&mut app, key(KeyCode::Tab));
    assert!(action.is_none());
}

// ── EditAgentRole (step 2: large floating editor) ─────────────────────────
//
// Cursor positions are char-indexed (handler uses `cursor_to_byte` to map to
// bytes for `String::insert`). Default 2 providers seeded by
// `test_app_isolated()` make `providers[provider_idx].label()` resolvable.

fn open_edit_agent_role(
    app: &mut App,
    editing_id: Option<i64>,
    name: &str,
    provider_idx: usize,
    role: &str,
    role_cursor: usize,
    scroll: usize,
) {
    app.mode = AppMode::EditAgentRole;
    app.active_dialog = Some(DialogState::EditAgentRole {
        editing_id,
        name: name.to_string(),
        provider_idx,
        role: role.to_string(),
        role_cursor,
        scroll,
    });
}

fn current_edit_agent_role(app: &App) -> (String, usize, usize) {
    match app.active_dialog {
        Some(DialogState::EditAgentRole {
            ref role,
            role_cursor,
            scroll,
            ..
        }) => (role.clone(), role_cursor, scroll),
        _ => panic!("expected EditAgentRole dialog"),
    }
}

#[test]
fn edit_agent_role_ctrl_s_saves_and_returns_to_manage_agents() {
    let (mut app, _tmp) = test_app_isolated();
    let providers = app.new_tab_agent_list();
    let expected_label = providers[1].label().to_string();
    open_edit_agent_role(&mut app, Some(42), "pilot", 1, "you are a pilot", 15, 0);

    let action = handle_edit_agent_role_input(
        &mut app,
        key_with_mods(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    match action {
        Some(Action::SaveAgent {
            source_repo,
            profile,
        }) => {
            // No active workspace in test_app — source_repo collapses to empty path.
            assert_eq!(source_repo, std::path::PathBuf::from(""));
            assert_eq!(profile.id, Some(42));
            assert_eq!(profile.name, "pilot");
            assert_eq!(profile.provider, expected_label);
            assert_eq!(profile.role, "you are a pilot");
        }
        other => panic!("expected SaveAgent, got {other:?}"),
    }
    assert_eq!(app.mode, AppMode::ManageAgents);
}

#[test]
fn edit_agent_role_ctrl_s_with_empty_role_still_saves() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, None, "pilot", 0, "", 0, 0);

    let action = handle_edit_agent_role_input(
        &mut app,
        key_with_mods(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    match action {
        Some(Action::SaveAgent { profile, .. }) => {
            assert_eq!(profile.role, "");
            assert_eq!(profile.id, None);
        }
        other => panic!("expected SaveAgent, got {other:?}"),
    }
}

#[test]
fn edit_agent_role_esc_returns_to_edit_agent_preserving_state() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, Some(9), "pilot", 1, "some role", 4, 0);

    let action = handle_edit_agent_role_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert_eq!(app.mode, AppMode::EditAgent);
    match app.active_dialog {
        Some(DialogState::EditAgent {
            editing_id,
            ref name,
            provider_idx,
            ref role,
            active_field,
            name_cursor,
        }) => {
            assert_eq!(editing_id, Some(9));
            assert_eq!(name, "pilot");
            assert_eq!(provider_idx, 1);
            assert_eq!(role, "some role");
            assert_eq!(name_cursor, "pilot".len());
            assert_eq!(active_field, EditAgentField::Name);
        }
        ref other => panic!("expected EditAgent, got {other:?}"),
    }
}

#[test]
fn edit_agent_role_ctrl_d_clears_role_and_resets_cursor() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, None, "pilot", 0, "hello\nworld", 5, 3);

    let action = handle_edit_agent_role_input(
        &mut app,
        key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL),
    );

    assert!(action.is_none());
    let (role, cursor, scroll) = current_edit_agent_role(&app);
    assert_eq!(role, "");
    assert_eq!(cursor, 0);
    assert_eq!(scroll, 0);
}

#[test]
fn edit_agent_role_enter_inserts_newline_at_cursor() {
    let (mut app, _tmp) = test_app_isolated();
    // Cursor at index 3 ("abc|def")
    open_edit_agent_role(&mut app, None, "pilot", 0, "abcdef", 3, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::Enter));

    let (role, cursor, _) = current_edit_agent_role(&app);
    assert_eq!(role, "abc\ndef");
    assert_eq!(cursor, 4);
}

#[test]
fn edit_agent_role_char_appends_at_cursor() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, None, "pilot", 0, "abc", 3, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::Char('X')));

    let (role, cursor, _) = current_edit_agent_role(&app);
    assert_eq!(role, "abcX");
    assert_eq!(cursor, 4);
}

#[test]
fn edit_agent_role_backspace_removes_previous_char() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, None, "pilot", 0, "abc", 3, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::Backspace));

    let (role, cursor, _) = current_edit_agent_role(&app);
    assert_eq!(role, "ab");
    assert_eq!(cursor, 2);
}

#[test]
fn edit_agent_role_tab_is_rejected() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, None, "pilot", 0, "abc", 3, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::Tab));

    let (role, cursor, _) = current_edit_agent_role(&app);
    assert_eq!(role, "abc");
    assert_eq!(cursor, 3);
}

#[test]
fn edit_agent_role_down_moves_cursor_to_next_line() {
    let (mut app, _tmp) = test_app_isolated();
    // "abc\ndef": cursor at col 2 on line 0
    open_edit_agent_role(&mut app, None, "pilot", 0, "abc\ndef", 2, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::Down));

    let (_, cursor, _) = current_edit_agent_role(&app);
    // Line 1 starts at char 4 ("d"); col 2 → cursor 6
    assert_eq!(cursor, 6);
}

#[test]
fn edit_agent_role_up_moves_cursor_to_previous_line() {
    let (mut app, _tmp) = test_app_isolated();
    // Cursor at col 2 on line 1
    open_edit_agent_role(&mut app, None, "pilot", 0, "abc\ndef", 6, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::Up));

    let (_, cursor, _) = current_edit_agent_role(&app);
    // Line 0 col 2 → cursor 2
    assert_eq!(cursor, 2);
}

#[test]
fn edit_agent_role_pageup_clamps_at_start() {
    let (mut app, _tmp) = test_app_isolated();
    // 3-line text, cursor in the middle line
    open_edit_agent_role(&mut app, None, "pilot", 0, "a\nb\nc", 2, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::PageUp));

    let (_, cursor, _) = current_edit_agent_role(&app);
    // PageUp jumps -10 lines → clamped to start
    assert_eq!(cursor, 0);
}

#[test]
fn edit_agent_role_pagedown_clamps_at_last_line() {
    let (mut app, _tmp) = test_app_isolated();
    open_edit_agent_role(&mut app, None, "pilot", 0, "a\nb\nc", 0, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::PageDown));

    let (_, cursor, _) = current_edit_agent_role(&app);
    // PageDown jumps +10 lines → clamped to last line; column 0 preserved →
    // cursor lands at the start of line 2 (`c`) which is char index 4.
    assert_eq!(cursor, 4);
}

#[test]
fn edit_agent_role_pagedown_from_last_line_jumps_to_end_of_text() {
    let (mut app, _tmp) = test_app_isolated();
    // Already on line 2 ("c"), col 0
    open_edit_agent_role(&mut app, None, "pilot", 0, "a\nb\nc", 4, 0);

    handle_edit_agent_role_input(&mut app, key(KeyCode::PageDown));

    let (_, cursor, _) = current_edit_agent_role(&app);
    // At boundary, delta > 0 → cursor jumps to end of text (char_count)
    assert_eq!(cursor, 5);
}

#[test]
fn edit_agent_role_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_edit_agent_role_input(&mut app, key(KeyCode::Enter));
    assert!(action.is_none());
}

// ── DispatchAgent ─────────────────────────────────────────────────────────
//
// Two-step dialog. Step 0 cycles `agent_idx` through `agents.len() +
// dispatchable_providers.len()` entries (Left/Right/Tab). Enter on step 0
// advances to step 1. Step 1 toggles `use_current_ws` via Left/Right/Tab,
// Enter dispatches an Action::DispatchAgent, Esc returns to step 0.

fn sample_dispatch_agents() -> Vec<(String, String, String)> {
    vec![
        ("alpha".to_string(), "Claude Code".to_string(), "role-a".to_string()),
        ("beta".to_string(), "Gemini".to_string(), "role-b".to_string()),
    ]
}

fn open_dispatch_agent(
    app: &mut App,
    agents: Vec<(String, String, String)>,
    step: u8,
    agent_idx: usize,
    use_current_ws: bool,
) {
    app.mode = AppMode::DispatchAgent;
    app.active_dialog = Some(DialogState::DispatchAgent {
        source_ws: 0,
        card_id: "CARD-7".to_string(),
        card_title: "Ship feature".to_string(),
        card_description: "Implement and test".to_string(),
        card_priority: flow_core::Priority::High,
        card_project: "piki".to_string(),
        agent_idx,
        agents,
        additional_prompt: String::new(),
        additional_prompt_cursor: 0,
        step,
        use_current_ws,
    });
}

fn current_dispatch_state(app: &App) -> (u8, usize, bool, String) {
    match app.active_dialog {
        Some(DialogState::DispatchAgent {
            step,
            agent_idx,
            use_current_ws,
            ref additional_prompt,
            ..
        }) => (step, agent_idx, use_current_ws, additional_prompt.clone()),
        _ => panic!("expected DispatchAgent dialog"),
    }
}

#[test]
fn dispatch_step0_right_cycles_forward_through_agents_then_providers() {
    let (mut app, _tmp) = test_app_isolated();
    let provider_count = app.dispatchable_provider_list().len();
    let agents = sample_dispatch_agents();
    let agent_count = agents.len();
    let total = agent_count + provider_count;
    open_dispatch_agent(&mut app, agents, 0, agent_count - 1, false);

    handle_dispatch_agent_input(&mut app, key(KeyCode::Right));
    // Crossed the agent/provider boundary
    assert_eq!(current_dispatch_state(&app).1, agent_count);

    // Wrap-around at the end
    open_dispatch_agent(
        &mut app,
        sample_dispatch_agents(),
        0,
        total - 1,
        false,
    );
    handle_dispatch_agent_input(&mut app, key(KeyCode::Right));
    assert_eq!(current_dispatch_state(&app).1, 0);
}

#[test]
fn dispatch_step0_tab_acts_as_right() {
    let (mut app, _tmp) = test_app_isolated();
    open_dispatch_agent(&mut app, sample_dispatch_agents(), 0, 0, false);

    handle_dispatch_agent_input(&mut app, key(KeyCode::Tab));
    assert_eq!(current_dispatch_state(&app).1, 1);
}

#[test]
fn dispatch_step0_left_cycles_backward_with_wrap() {
    let (mut app, _tmp) = test_app_isolated();
    let provider_count = app.dispatchable_provider_list().len();
    let agents = sample_dispatch_agents();
    let total = agents.len() + provider_count;
    open_dispatch_agent(&mut app, agents, 0, 0, false);

    handle_dispatch_agent_input(&mut app, key(KeyCode::Left));
    // Wraps to last (provider) entry
    assert_eq!(current_dispatch_state(&app).1, total - 1);
}

#[test]
fn dispatch_step0_typed_chars_append_to_additional_prompt() {
    let (mut app, _tmp) = test_app_isolated();
    open_dispatch_agent(&mut app, sample_dispatch_agents(), 0, 0, false);

    for c in "hi!".chars() {
        handle_dispatch_agent_input(&mut app, key(KeyCode::Char(c)));
    }

    let (_, _, _, prompt) = current_dispatch_state(&app);
    assert_eq!(prompt, "hi!");
}

#[test]
fn dispatch_step0_enter_advances_to_step1_without_action() {
    let (mut app, _tmp) = test_app_isolated();
    open_dispatch_agent(&mut app, sample_dispatch_agents(), 0, 0, false);

    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert_eq!(current_dispatch_state(&app).0, 1);
}

#[test]
fn dispatch_step0_esc_dismisses() {
    let (mut app, _tmp) = test_app_isolated();
    open_dispatch_agent(&mut app, sample_dispatch_agents(), 0, 0, false);

    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn dispatch_step1_left_right_tab_toggle_use_current_ws() {
    let (mut app, _tmp) = test_app_isolated();
    open_dispatch_agent(&mut app, sample_dispatch_agents(), 1, 0, false);

    handle_dispatch_agent_input(&mut app, key(KeyCode::Right));
    assert!(current_dispatch_state(&app).2);
    handle_dispatch_agent_input(&mut app, key(KeyCode::Left));
    assert!(!current_dispatch_state(&app).2);
    handle_dispatch_agent_input(&mut app, key(KeyCode::Tab));
    assert!(current_dispatch_state(&app).2);
}

#[test]
fn dispatch_step1_enter_with_agent_dispatches_with_resolved_name_and_role() {
    let (mut app, _tmp) = test_app_isolated();
    let agents = sample_dispatch_agents();
    // Select the second agent (idx=1: "beta", Gemini, role-b)
    open_dispatch_agent(&mut app, agents, 1, 1, true);

    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::DispatchAgent {
            source_ws,
            card_id,
            card_title,
            card_priority,
            card_project,
            provider,
            agent_name,
            agent_role,
            use_current_ws,
            ..
        }) => {
            assert_eq!(source_ws, 0);
            assert_eq!(card_id, "CARD-7");
            assert_eq!(card_title, "Ship feature");
            assert_eq!(card_priority, flow_core::Priority::High);
            assert_eq!(card_project, "piki");
            assert_eq!(provider, AIProvider::Custom("Gemini".to_string()));
            assert_eq!(agent_name, Some("beta".to_string()));
            assert_eq!(agent_role, Some("role-b".to_string()));
            assert!(use_current_ws);
        }
        other => panic!("expected DispatchAgent action, got {other:?}"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn dispatch_step1_enter_with_provider_only_dispatches_without_name_or_role() {
    let (mut app, _tmp) = test_app_isolated();
    let agents = sample_dispatch_agents();
    let agent_count = agents.len();
    let providers = app.dispatchable_provider_list();
    let first_provider = providers[0].clone();
    // Select the first provider entry (idx == agent_count)
    open_dispatch_agent(&mut app, agents, 1, agent_count, false);

    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::DispatchAgent {
            provider,
            agent_name,
            agent_role,
            use_current_ws,
            ..
        }) => {
            assert_eq!(provider, first_provider);
            assert_eq!(agent_name, None);
            assert_eq!(agent_role, None);
            assert!(!use_current_ws);
        }
        other => panic!("expected DispatchAgent, got {other:?}"),
    }
}

#[test]
fn dispatch_step1_esc_returns_to_step0() {
    let (mut app, _tmp) = test_app_isolated();
    open_dispatch_agent(&mut app, sample_dispatch_agents(), 1, 0, true);

    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    let (step, _, use_current_ws, _) = current_dispatch_state(&app);
    assert_eq!(step, 0);
    // use_current_ws is NOT reset when going back
    assert!(use_current_ws);
}

#[test]
fn dispatch_step0_with_no_entries_at_all_is_noop() {
    let (mut app, _tmp) = test_app_isolated();
    // Drain all providers so total = 0 when agents is also empty
    let names: Vec<String> = app
        .provider_manager
        .all()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    for name in &names {
        app.provider_manager.remove(name);
    }
    assert_eq!(app.dispatchable_provider_list().len(), 0);
    open_dispatch_agent(&mut app, vec![], 0, 0, false);

    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Right));

    assert!(action.is_none());
    // Dialog still open
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::DispatchAgent { .. })
    ));
}

#[test]
fn dispatch_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_dispatch_agent_input(&mut app, key(KeyCode::Enter));
    assert!(action.is_none());
}

// ── NewWorkspace dialog (Layer 2 redesign) ────────────────────────────────
//
// The dialog has a Source toggle (Local | GitHub) and 7 fields total:
// Source → Directory → Name → Description → Prompt → KanbanPath → Group.
// Enter dispatches either `Action::CreateWorkspace` (Simple type, Local
// source) or `Action::CreateGithubWorkspace` (GitHub source). Workspace
// name is auto-derived when the Name field is empty.

use crate::app::NewWorkspaceSource;
use crate::dialog_state::CycleField;

fn open_new_workspace(app: &mut App, source: NewWorkspaceSource, active_field: DialogField) {
    app.mode = AppMode::NewWorkspace;
    app.active_pane = ActivePane::WorkspaceList;
    app.active_dialog = Some(DialogState::NewWorkspace {
        name: String::new(),
        name_cursor: 0,
        dir: String::new(),
        dir_cursor: 0,
        desc: String::new(),
        desc_cursor: 0,
        prompt: String::new(),
        prompt_cursor: 0,
        kanban: String::new(),
        kanban_cursor: 0,
        group: String::new(),
        group_cursor: 0,
        source,
        active_field,
    });
}

fn current_new_workspace_field(app: &App) -> DialogField {
    match app.active_dialog {
        Some(DialogState::NewWorkspace { active_field, .. }) => active_field,
        _ => panic!("expected NewWorkspace dialog"),
    }
}

fn current_new_workspace_source(app: &App) -> NewWorkspaceSource {
    match app.active_dialog {
        Some(DialogState::NewWorkspace { source, .. }) => source,
        _ => panic!("expected NewWorkspace dialog"),
    }
}

fn current_new_workspace_text(app: &App, field: DialogField) -> String {
    match app.active_dialog {
        Some(DialogState::NewWorkspace {
            ref name,
            ref dir,
            ref desc,
            ref prompt,
            ref kanban,
            ref group,
            ..
        }) => match field {
            DialogField::Name => name.clone(),
            DialogField::Directory => dir.clone(),
            DialogField::Description => desc.clone(),
            DialogField::Prompt => prompt.clone(),
            DialogField::KanbanPath => kanban.clone(),
            DialogField::Group => group.clone(),
            DialogField::Source => panic!("Source field has no text buffer"),
        },
        _ => panic!("expected NewWorkspace dialog"),
    }
}

fn set_new_workspace_buffer(app: &mut App, field: DialogField, value: &str) {
    match app.active_dialog {
        Some(DialogState::NewWorkspace {
            ref mut name,
            ref mut name_cursor,
            ref mut dir,
            ref mut dir_cursor,
            ref mut desc,
            ref mut desc_cursor,
            ref mut prompt,
            ref mut prompt_cursor,
            ref mut kanban,
            ref mut kanban_cursor,
            ref mut group,
            ref mut group_cursor,
            ..
        }) => {
            let (buf, cursor) = match field {
                DialogField::Name => (name, name_cursor),
                DialogField::Directory => (dir, dir_cursor),
                DialogField::Description => (desc, desc_cursor),
                DialogField::Prompt => (prompt, prompt_cursor),
                DialogField::KanbanPath => (kanban, kanban_cursor),
                DialogField::Group => (group, group_cursor),
                DialogField::Source => return,
            };
            *buf = value.to_string();
            *cursor = value.chars().count();
        }
        _ => panic!("expected NewWorkspace dialog"),
    }
}

// ── Unit tests for the CycleField impl on DialogField ─────────────────────

#[test]
fn cycle_field_next_visits_all_seven_fields() {
    let order = [
        DialogField::Source,
        DialogField::Directory,
        DialogField::Name,
        DialogField::Description,
        DialogField::Prompt,
        DialogField::KanbanPath,
        DialogField::Group,
    ];
    for w in order.windows(2) {
        assert_eq!(w[0].next(), w[1], "next({:?})", w[0]);
    }
    assert_eq!(DialogField::Group.next(), DialogField::Source);
}

#[test]
fn cycle_field_prev_is_inverse_of_next() {
    let all = [
        DialogField::Source,
        DialogField::Directory,
        DialogField::Name,
        DialogField::Description,
        DialogField::Prompt,
        DialogField::KanbanPath,
        DialogField::Group,
    ];
    for f in all {
        assert_eq!(f.next().prev(), f, "prev(next({:?}))", f);
        assert_eq!(f.prev().next(), f, "next(prev({:?}))", f);
    }
}

// ── Handler tests ─────────────────────────────────────────────────────────

#[test]
fn new_workspace_tab_full_cycle() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Source);

    let expected = [
        DialogField::Directory,
        DialogField::Name,
        DialogField::Description,
        DialogField::Prompt,
        DialogField::KanbanPath,
        DialogField::Group,
        DialogField::Source,
    ];
    for target in expected {
        handle_new_workspace_input(&mut app, key(KeyCode::Tab));
        assert_eq!(current_new_workspace_field(&app), target);
    }
}

#[test]
fn new_workspace_source_field_space_toggles_local_github() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Source);

    handle_new_workspace_input(&mut app, key(KeyCode::Char(' ')));
    assert_eq!(current_new_workspace_source(&app), NewWorkspaceSource::GitHub);

    handle_new_workspace_input(&mut app, key(KeyCode::Char(' ')));
    assert_eq!(current_new_workspace_source(&app), NewWorkspaceSource::Local);
}

#[test]
fn new_workspace_source_field_right_and_left_also_toggle() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Source);

    handle_new_workspace_input(&mut app, key(KeyCode::Right));
    assert_eq!(current_new_workspace_source(&app), NewWorkspaceSource::GitHub);

    handle_new_workspace_input(&mut app, key(KeyCode::Left));
    assert_eq!(current_new_workspace_source(&app), NewWorkspaceSource::Local);
}

#[test]
fn new_workspace_source_toggle_clears_dir_buffer() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Source);
    set_new_workspace_buffer(&mut app, DialogField::Directory, "/some/path");
    assert_eq!(
        current_new_workspace_text(&app, DialogField::Directory),
        "/some/path"
    );

    handle_new_workspace_input(&mut app, key(KeyCode::Char(' ')));
    assert_eq!(current_new_workspace_text(&app, DialogField::Directory), "");
}

#[test]
fn new_workspace_name_field_accepts_alphanumeric_dash_underscore_dot_slash() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Name);

    for c in "feat-1_v.2/x".chars() {
        handle_new_workspace_input(&mut app, key(KeyCode::Char(c)));
    }
    assert_eq!(
        current_new_workspace_text(&app, DialogField::Name),
        "feat-1_v.2/x"
    );
}

#[test]
fn new_workspace_name_field_rejects_punctuation() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Name);

    for c in "a b!c@".chars() {
        handle_new_workspace_input(&mut app, key(KeyCode::Char(c)));
    }
    assert_eq!(current_new_workspace_text(&app, DialogField::Name), "abc");
}

#[test]
fn new_workspace_description_field_accepts_any_non_control_char() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Description);

    for c in "Hello, world! 🚀".chars() {
        handle_new_workspace_input(&mut app, key(KeyCode::Char(c)));
    }
    assert_eq!(
        current_new_workspace_text(&app, DialogField::Description),
        "Hello, world! 🚀"
    );
}

#[test]
fn new_workspace_enter_with_empty_source_keeps_dialog_open() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Directory);

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::NewWorkspace { .. })
    ));
    assert!(app.status_message.is_some());
}

#[test]
fn new_workspace_enter_with_nonexistent_local_folder_keeps_dialog_open() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Directory);
    set_new_workspace_buffer(
        &mut app,
        DialogField::Directory,
        "/this/path/does/not/exist/zzz",
    );

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::NewWorkspace { .. })
    ));
    assert!(
        app.status_message
            .as_deref()
            .is_some_and(|m| m.contains("does not exist"))
    );
}

#[test]
fn new_workspace_enter_local_with_explicit_name_dispatches_create() {
    let mut app = test_app();
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dir_str = tmp.path().to_string_lossy().to_string();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Group);
    set_new_workspace_buffer(&mut app, DialogField::Name, "my-ws");
    set_new_workspace_buffer(&mut app, DialogField::Directory, &dir_str);
    set_new_workspace_buffer(&mut app, DialogField::Description, "desc");
    set_new_workspace_buffer(&mut app, DialogField::Prompt, "go");
    set_new_workspace_buffer(&mut app, DialogField::KanbanPath, "kanban.toml");
    set_new_workspace_buffer(&mut app, DialogField::Group, "backend");

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::CreateWorkspace(
            name,
            desc,
            prompt,
            kanban,
            dir_path,
            ws_type,
            group,
        )) => {
            assert_eq!(name, "my-ws");
            assert_eq!(desc, "desc");
            assert_eq!(prompt, "go");
            assert_eq!(kanban.as_deref(), Some("kanban.toml"));
            assert_eq!(dir_path, std::path::PathBuf::from(&dir_str));
            assert_eq!(ws_type, WorkspaceType::Simple);
            assert_eq!(group.as_deref(), Some("backend"));
        }
        other => panic!("expected CreateWorkspace, got {other:?}"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn new_workspace_enter_local_derives_name_from_folder_basename() {
    let mut app = test_app();
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dir_path = tmp.path().to_path_buf();
    let basename = dir_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Directory);
    set_new_workspace_buffer(
        &mut app,
        DialogField::Directory,
        &dir_path.to_string_lossy(),
    );

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::CreateWorkspace(name, _, _, _, _, ws_type, _)) => {
            assert_eq!(name, basename);
            assert_eq!(ws_type, WorkspaceType::Simple);
        }
        other => panic!("expected CreateWorkspace, got {other:?}"),
    }
}

#[test]
fn new_workspace_enter_local_expands_tilde() {
    let home =
        std::env::var("HOME").expect("HOME must be set for tilde-expansion test");
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Directory);
    set_new_workspace_buffer(&mut app, DialogField::Directory, "~");

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::CreateWorkspace(_, _, _, _, dir_path, _, _)) => {
            assert_eq!(dir_path, std::path::PathBuf::from(&home));
        }
        other => panic!("expected CreateWorkspace, got {other:?}"),
    }
}

#[test]
fn new_workspace_enter_github_dispatches_create_github_action() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::GitHub, DialogField::Directory);
    set_new_workspace_buffer(
        &mut app,
        DialogField::Directory,
        "https://github.com/owner/myrepo.git",
    );
    set_new_workspace_buffer(&mut app, DialogField::Description, "d");
    set_new_workspace_buffer(&mut app, DialogField::Prompt, "p");
    set_new_workspace_buffer(&mut app, DialogField::Group, "g");

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::CreateGithubWorkspace(name, desc, prompt, kanban, url, group)) => {
            assert_eq!(name, "myrepo");
            assert_eq!(desc, "d");
            assert_eq!(prompt, "p");
            assert!(kanban.is_none());
            assert_eq!(url, "https://github.com/owner/myrepo.git");
            assert_eq!(group.as_deref(), Some("g"));
        }
        other => panic!("expected CreateGithubWorkspace, got {other:?}"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn new_workspace_enter_github_with_explicit_name_overrides_auto_derive() {
    let mut app = test_app();
    open_new_workspace(&mut app, NewWorkspaceSource::GitHub, DialogField::Name);
    set_new_workspace_buffer(&mut app, DialogField::Name, "custom-name");
    set_new_workspace_buffer(
        &mut app,
        DialogField::Directory,
        "https://github.com/owner/repo",
    );

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::CreateGithubWorkspace(name, ..)) => {
            assert_eq!(name, "custom-name");
        }
        other => panic!("expected CreateGithubWorkspace, got {other:?}"),
    }
}

#[test]
fn new_workspace_esc_dismisses_and_focuses_workspace_list() {
    let mut app = test_app();
    app.active_pane = ActivePane::MainPanel;
    open_new_workspace(&mut app, NewWorkspaceSource::Local, DialogField::Name);

    let action = handle_new_workspace_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.active_pane, ActivePane::WorkspaceList);
}

#[test]
fn new_workspace_returns_none_when_dialog_not_active() {
    let mut app = test_app();
    let action = handle_new_workspace_input(&mut app, key(KeyCode::Tab));
    assert!(action.is_none());
}

// ── Layer 3: CreateWorktree dialog + clone_workspace keybinding gating ────

use crate::app::Workspace;
use crate::dialog_state::CreateWorktreeField;
use piki_core::WorkspaceOrigin;
use super::dialog::handle_create_worktree_input;

fn push_test_ws(app: &mut App, name: &str, origin: WorkspaceOrigin) -> usize {
    let mut info = piki_core::WorkspaceInfo::new(
        name.to_string(),
        String::new(),
        String::new(),
        None,
        "main".to_string(),
        std::path::PathBuf::from("/tmp/test"),
        std::path::PathBuf::from("/tmp/test/parent"),
    );
    info.origin = origin;
    info.workspace_type = piki_core::WorkspaceType::Simple;
    app.workspaces.push(Workspace::from_info(info));
    app.workspaces.len() - 1
}

#[test]
fn clone_keybinding_on_github_workspace_opens_create_worktree() {
    let mut app = test_app();
    let idx = push_test_ws(
        &mut app,
        "gh-ws",
        WorkspaceOrigin::GitHub {
            url: "https://github.com/owner/repo".into(),
        },
    );
    app.selected_workspace = idx;

    crate::input::handle_key_event(&mut app, key(KeyCode::Char('r')));

    assert_eq!(app.mode, AppMode::CreateWorktree);
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::CreateWorktree { .. })
    ));
}

#[test]
fn clone_keybinding_on_local_workspace_shows_status_message() {
    let mut app = test_app();
    let idx = push_test_ws(&mut app, "local-ws", WorkspaceOrigin::Local);
    app.selected_workspace = idx;

    crate::input::handle_key_event(&mut app, key(KeyCode::Char('r')));

    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.active_dialog.is_none());
    assert!(
        app.status_message
            .as_deref()
            .is_some_and(|m| m.contains("GitHub"))
    );
}

#[test]
fn create_worktree_tab_cycles_four_fields() {
    let mut app = test_app();
    let idx = push_test_ws(
        &mut app,
        "gh",
        WorkspaceOrigin::GitHub {
            url: "https://github.com/o/r".into(),
        },
    );
    app.active_dialog = Some(DialogState::CreateWorktree {
        parent_idx: idx,
        name: String::new(),
        name_cursor: 0,
        prompt: String::new(),
        prompt_cursor: 0,
        kanban: String::new(),
        kanban_cursor: 0,
        group: String::new(),
        group_cursor: 0,
        active_field: CreateWorktreeField::Name,
    });
    app.mode = AppMode::CreateWorktree;

    let cycle = [
        CreateWorktreeField::Prompt,
        CreateWorktreeField::KanbanPath,
        CreateWorktreeField::Group,
        CreateWorktreeField::Name,
    ];
    for expected in cycle {
        handle_create_worktree_input(&mut app, key(KeyCode::Tab));
        let actual = match app.active_dialog {
            Some(DialogState::CreateWorktree { active_field, .. }) => active_field,
            _ => panic!("expected CreateWorktree dialog"),
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn create_worktree_enter_with_empty_name_keeps_dialog_open() {
    let mut app = test_app();
    let idx = push_test_ws(
        &mut app,
        "gh",
        WorkspaceOrigin::GitHub {
            url: "https://github.com/o/r".into(),
        },
    );
    app.active_dialog = Some(DialogState::CreateWorktree {
        parent_idx: idx,
        name: String::new(),
        name_cursor: 0,
        prompt: String::new(),
        prompt_cursor: 0,
        kanban: String::new(),
        kanban_cursor: 0,
        group: String::new(),
        group_cursor: 0,
        active_field: CreateWorktreeField::Name,
    });
    app.mode = AppMode::CreateWorktree;

    let action = handle_create_worktree_input(&mut app, key(KeyCode::Enter));

    assert!(action.is_none());
    assert!(matches!(
        app.active_dialog,
        Some(DialogState::CreateWorktree { .. })
    ));
    assert!(app.status_message.is_some());
}

#[test]
fn create_worktree_enter_dispatches_create_workspace_with_worktree_type() {
    let mut app = test_app();
    let parent_repo = std::path::PathBuf::from("/tmp/parent-repo");
    let idx = {
        let mut info = piki_core::WorkspaceInfo::new(
            "gh-parent".into(),
            String::new(),
            String::new(),
            None,
            "main".into(),
            parent_repo.clone(),
            parent_repo.clone(),
        );
        info.origin = WorkspaceOrigin::GitHub {
            url: "https://github.com/o/r".into(),
        };
        app.workspaces.push(Workspace::from_info(info));
        app.workspaces.len() - 1
    };
    app.active_dialog = Some(DialogState::CreateWorktree {
        parent_idx: idx,
        name: "feature/x".into(),
        name_cursor: 0,
        prompt: "do the thing".into(),
        prompt_cursor: 0,
        kanban: "  ".into(),
        kanban_cursor: 0,
        group: "agents".into(),
        group_cursor: 0,
        active_field: CreateWorktreeField::Name,
    });
    app.mode = AppMode::CreateWorktree;

    let action = handle_create_worktree_input(&mut app, key(KeyCode::Enter));

    match action {
        Some(Action::CreateWorkspace(name, _, prompt, kanban, dir, ws_type, group)) => {
            assert_eq!(name, "feature/x");
            assert_eq!(prompt, "do the thing");
            assert!(kanban.is_none());
            assert_eq!(dir, parent_repo);
            assert_eq!(ws_type, WorkspaceType::Worktree);
            assert_eq!(group.as_deref(), Some("agents"));
        }
        other => panic!("expected CreateWorkspace(Worktree), got {other:?}"),
    }
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn create_worktree_esc_dismisses() {
    let mut app = test_app();
    let idx = push_test_ws(
        &mut app,
        "gh",
        WorkspaceOrigin::GitHub {
            url: "https://github.com/o/r".into(),
        },
    );
    app.active_dialog = Some(DialogState::CreateWorktree {
        parent_idx: idx,
        name: String::new(),
        name_cursor: 0,
        prompt: String::new(),
        prompt_cursor: 0,
        kanban: String::new(),
        kanban_cursor: 0,
        group: String::new(),
        group_cursor: 0,
        active_field: CreateWorktreeField::Name,
    });
    app.mode = AppMode::CreateWorktree;

    let action = handle_create_worktree_input(&mut app, key(KeyCode::Esc));

    assert!(action.is_none());
    assert!(app.active_dialog.is_none());
    assert_eq!(app.mode, AppMode::Normal);
}
