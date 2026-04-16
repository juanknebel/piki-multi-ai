use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{App, AppMode, ChatSettingsField, ChatSubMode};

pub(super) fn handle_chat_panel_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Handle pending approval dialog (y/n/a)
    if app.chat_panel.pending_approval.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(req) = app.chat_panel.pending_approval.take() {
                    let _ = req.response_tx.send(piki_agent::ApprovalResponse::Allow);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                if let Some(req) = app.chat_panel.pending_approval.take() {
                    let _ = req.response_tx.send(piki_agent::ApprovalResponse::Deny);
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let Some(req) = app.chat_panel.pending_approval.take() {
                    let _ = req.response_tx.send(piki_agent::ApprovalResponse::AllowAll);
                    app.set_toast("Auto-approve enabled for this session", crate::app::ToastLevel::Info);
                }
            }
            _ => {}
        }
        return None;
    }

    match app.chat_panel.sub_mode {
        ChatSubMode::ModelSelect => return handle_model_select(app, key),
        ChatSubMode::Settings => return handle_settings(app, key),
        ChatSubMode::Chat => {}
    }

    match key.code {
        KeyCode::Esc => {
            // Hide overlay — state is preserved
            app.mode = AppMode::Normal;
        }
        KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            if !app.chat_panel.streaming && !app.chat_panel.input.trim().is_empty() {
                return Some(Action::ChatSendMessage);
            }
        }
        KeyCode::Tab => {
            // Open model selector
            if !app.chat_panel.models.is_empty() {
                app.chat_panel.sub_mode = ChatSubMode::ModelSelect;
                // Pre-select the current model
                if let Some(pos) = app
                    .chat_panel
                    .models
                    .iter()
                    .position(|m| *m == app.chat_panel.config.model)
                {
                    app.chat_panel.model_selected = pos;
                }
            } else {
                // Try to load models
                return Some(Action::ChatLoadModels);
            }
        }
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Open settings
            app.chat_panel.settings_server_type = app.chat_panel.config.server_type;
            app.chat_panel.settings_url = app.chat_panel.config.base_url.clone();
            app.chat_panel.settings_prompt =
                app.chat_panel.config.system_prompt.clone().unwrap_or_default();
            app.chat_panel.settings_field = ChatSettingsField::ServerType;
            app.chat_panel.settings_cursor = 0;
            app.chat_panel.sub_mode = ChatSubMode::Settings;
        }
        KeyCode::Up => {
            app.chat_panel.scroll = app.chat_panel.scroll.saturating_add(1);
        }
        KeyCode::Down => {
            app.chat_panel.scroll = app.chat_panel.scroll.saturating_sub(1);
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chat_panel.messages.clear();
            app.chat_panel.current_response.clear();
            app.chat_panel.scroll = 0;
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chat_panel.agent_mode = !app.chat_panel.agent_mode;
            let label = if app.chat_panel.agent_mode {
                "Agent mode ON"
            } else {
                "Agent mode OFF"
            };
            app.set_toast(label, crate::app::ToastLevel::Info);
        }
        KeyCode::Char(c) => {
            app.chat_panel.input.insert(app.chat_panel.input_cursor, c);
            app.chat_panel.input_cursor += c.len_utf8();
        }
        KeyCode::Backspace => {
            if app.chat_panel.input_cursor > 0 {
                let prev = prev_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
                app.chat_panel.input.drain(prev..app.chat_panel.input_cursor);
                app.chat_panel.input_cursor = prev;
            }
        }
        KeyCode::Delete => {
            if app.chat_panel.input_cursor < app.chat_panel.input.len() {
                let next = next_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
                app.chat_panel.input.drain(app.chat_panel.input_cursor..next);
            }
        }
        KeyCode::Left => {
            if app.chat_panel.input_cursor > 0 {
                app.chat_panel.input_cursor =
                    prev_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
            }
        }
        KeyCode::Right => {
            if app.chat_panel.input_cursor < app.chat_panel.input.len() {
                app.chat_panel.input_cursor =
                    next_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
            }
        }
        KeyCode::Home => {
            app.chat_panel.input_cursor = 0;
        }
        KeyCode::End => {
            app.chat_panel.input_cursor = app.chat_panel.input.len();
        }
        _ => {}
    }

    None
}

fn handle_model_select(app: &mut App, key: KeyEvent) -> Option<Action> {
    let total = app.chat_panel.models.len();
    match key.code {
        KeyCode::Esc | KeyCode::Tab => {
            app.chat_panel.sub_mode = ChatSubMode::Chat;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.chat_panel.model_selected > 0 {
                app.chat_panel.model_selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.chat_panel.model_selected + 1 < total {
                app.chat_panel.model_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(name) = app.chat_panel.models.get(app.chat_panel.model_selected) {
                app.chat_panel.config.model = name.clone();
                save_chat_config(app);
            }
            app.chat_panel.sub_mode = ChatSubMode::Chat;
        }
        _ => {}
    }
    None
}

fn handle_settings(app: &mut App, key: KeyEvent) -> Option<Action> {
    // ServerType field only responds to Enter/Space (toggle) and Tab/arrows (navigate)
    if app.chat_panel.settings_field == ChatSettingsField::ServerType {
        match key.code {
            KeyCode::Esc => {
                app.chat_panel.sub_mode = ChatSubMode::Chat;
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return save_and_close_settings(app);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Cycle server type and update default URL if it matches the old default
                let old_type = app.chat_panel.settings_server_type;
                let new_type = old_type.next();
                let old_default = old_type.default_url();
                app.chat_panel.settings_server_type = new_type;
                // If URL was the old default, swap to new default
                if app.chat_panel.settings_url == old_default {
                    app.chat_panel.settings_url = new_type.default_url().to_string();
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                app.chat_panel.settings_field = ChatSettingsField::BaseUrl;
                app.chat_panel.settings_cursor = app.chat_panel.settings_url.len();
            }
            KeyCode::Up => {
                app.chat_panel.settings_field = ChatSettingsField::SystemPrompt;
                app.chat_panel.settings_cursor = app.chat_panel.settings_prompt.len();
            }
            _ => {}
        }
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            // Discard edits
            app.chat_panel.sub_mode = ChatSubMode::Chat;
        }
        KeyCode::Tab | KeyCode::Down => {
            // Cycle forward: BaseUrl -> SystemPrompt -> ServerType -> ...
            let (new_field, new_cursor) = match app.chat_panel.settings_field {
                ChatSettingsField::ServerType => {
                    (ChatSettingsField::BaseUrl, app.chat_panel.settings_url.len())
                }
                ChatSettingsField::BaseUrl => {
                    (ChatSettingsField::SystemPrompt, app.chat_panel.settings_prompt.len())
                }
                ChatSettingsField::SystemPrompt => {
                    (ChatSettingsField::ServerType, 0)
                }
            };
            app.chat_panel.settings_field = new_field;
            app.chat_panel.settings_cursor = new_cursor;
        }
        KeyCode::Up => {
            // Cycle backward: SystemPrompt -> BaseUrl -> ServerType -> ...
            let (new_field, new_cursor) = match app.chat_panel.settings_field {
                ChatSettingsField::ServerType => {
                    (ChatSettingsField::SystemPrompt, app.chat_panel.settings_prompt.len())
                }
                ChatSettingsField::BaseUrl => {
                    (ChatSettingsField::ServerType, 0)
                }
                ChatSettingsField::SystemPrompt => {
                    (ChatSettingsField::BaseUrl, app.chat_panel.settings_url.len())
                }
            };
            app.chat_panel.settings_field = new_field;
            app.chat_panel.settings_cursor = new_cursor;
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return save_and_close_settings(app);
        }
        KeyCode::Char(c) => {
            let (field, cursor) = active_field_mut(app);
            field.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
        KeyCode::Backspace => {
            let (field, cursor) = active_field_mut(app);
            if *cursor > 0 {
                let prev = prev_char_boundary(field, *cursor);
                field.drain(prev..*cursor);
                *cursor = prev;
            }
        }
        KeyCode::Left => {
            let (field, cursor) = active_field_mut(app);
            if *cursor > 0 {
                *cursor = prev_char_boundary(field, *cursor);
            }
        }
        KeyCode::Right => {
            let (field, cursor) = active_field_mut(app);
            let len = field.len();
            if *cursor < len {
                *cursor = next_char_boundary(field, *cursor);
            }
        }
        KeyCode::Home => {
            app.chat_panel.settings_cursor = 0;
        }
        KeyCode::End => {
            let len = match app.chat_panel.settings_field {
                ChatSettingsField::ServerType => 0,
                ChatSettingsField::BaseUrl => app.chat_panel.settings_url.len(),
                ChatSettingsField::SystemPrompt => app.chat_panel.settings_prompt.len(),
            };
            app.chat_panel.settings_cursor = len;
        }
        _ => {}
    }
    None
}

/// Save settings and close the settings sub-mode. Returns an action if models need reloading.
fn save_and_close_settings(app: &mut App) -> Option<Action> {
    let url = app.chat_panel.settings_url.trim().to_string();
    let prompt = app.chat_panel.settings_prompt.trim().to_string();
    let new_server_type = app.chat_panel.settings_server_type;
    let server_changed = new_server_type != app.chat_panel.config.server_type;
    let url_changed = url != app.chat_panel.config.base_url;

    app.chat_panel.config.server_type = new_server_type;
    app.chat_panel.config.base_url = if url.is_empty() {
        new_server_type.default_url().to_string()
    } else {
        url
    };
    app.chat_panel.config.system_prompt = if prompt.is_empty() {
        None
    } else {
        Some(prompt)
    };

    if server_changed {
        // Clear model on server type change since model names differ
        app.chat_panel.config.model.clear();
    }

    save_chat_config(app);
    app.chat_panel.sub_mode = ChatSubMode::Chat;

    if url_changed || server_changed {
        // Clear models so they reload from the new URL/server
        app.chat_panel.models.clear();
        return Some(Action::ChatLoadModels);
    }
    None
}

/// Get a mutable reference to the active settings field and its cursor.
///
/// Only called for text-editable fields (BaseUrl, SystemPrompt).
/// ServerType is handled separately and never reaches this path.
fn active_field_mut(app: &mut App) -> (&mut String, &mut usize) {
    let cursor = &mut app.chat_panel.settings_cursor as *mut usize;
    let field = match app.chat_panel.settings_field {
        ChatSettingsField::ServerType | ChatSettingsField::BaseUrl => {
            &mut app.chat_panel.settings_url
        }
        ChatSettingsField::SystemPrompt => &mut app.chat_panel.settings_prompt,
    };
    // SAFETY: cursor and field point to different fields of the same struct
    (field, unsafe { &mut *cursor })
}

fn save_chat_config(app: &App) {
    if let Some(ref ui_prefs) = app.storage.ui_prefs
        && let Ok(json) = serde_json::to_string(&app.chat_panel.config)
    {
        let _ = ui_prefs.set_preference("chat_config", &json);
    }
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.saturating_sub(1);
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}
