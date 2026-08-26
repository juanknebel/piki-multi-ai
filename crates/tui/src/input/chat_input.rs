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
                    app.set_toast(
                        "Auto-approve enabled for this session",
                        crate::app::ToastLevel::Info,
                    );
                }
            }
            _ => {}
        }
        return None;
    }

    // Copy/paste via system clipboard (Ctrl+Shift+C / Ctrl+Shift+V) — same as terminal pane
    if app.config.matches_app_direct(key, "copy") {
        let text = match app.chat_panel.sub_mode {
            ChatSubMode::ModelSelect => {
                // Copy selected model name
                let filtered = filtered_indices(app);
                filtered
                    .get(app.chat_panel.model_selected)
                    .and_then(|&idx| app.chat_panel.models.get(idx).cloned())
                    .unwrap_or_default()
            }
            ChatSubMode::Settings => String::new(),
            ChatSubMode::Chat => {
                if app.chat_panel.streaming && !app.chat_panel.current_response.is_empty() {
                    app.chat_panel.current_response.clone()
                } else if !app.chat_panel.input.is_empty() {
                    app.chat_panel.input.clone()
                } else if let Some(last) = app
                    .chat_panel
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == piki_core::chat::ChatRole::Assistant)
                {
                    last.content.clone()
                } else if let Some(last) = app.chat_panel.messages.last() {
                    last.content.clone()
                } else {
                    String::new()
                }
            }
        };
        if text.trim().is_empty() {
            app.set_toast("Nothing to copy", crate::app::ToastLevel::Info);
        } else {
            match crate::clipboard::copy_to_clipboard(&text) {
                Ok(()) => app.set_toast("Copied to clipboard", crate::app::ToastLevel::Info),
                Err(e) => app.set_toast(format!("Copy failed: {e}"), crate::app::ToastLevel::Error),
            }
        }
        return None;
    }
    if app.config.matches_app_direct(key, "paste") {
        match crate::clipboard::paste_from_clipboard() {
            Ok(text) if !text.is_empty() => match app.chat_panel.sub_mode {
                ChatSubMode::Chat => {
                    let cursor = app.chat_panel.input_cursor;
                    let byte_pos = {
                        let mut b = 0;
                        let mut c = 0;
                        for (bi, _) in app.chat_panel.input.char_indices() {
                            if c == cursor {
                                b = bi;
                                break;
                            }
                            c += 1;
                        }
                        if c == cursor {
                            app.chat_panel.input.len()
                        } else {
                            b
                        }
                    };
                    app.chat_panel.input.insert_str(byte_pos, &text);
                    app.chat_panel.input_cursor += text.chars().count();
                }
                ChatSubMode::ModelSelect => {
                    app.chat_panel.model_filter.push_str(&text);
                    app.chat_panel.model_selected = 0;
                }
                ChatSubMode::Settings => {
                    let (field, cursor) = active_field_mut(app);
                    let byte_pos = {
                        let mut b = 0;
                        let mut c = 0;
                        for (bi, _) in field.char_indices() {
                            if c == *cursor {
                                b = bi;
                                break;
                            }
                            c += 1;
                        }
                        if c == *cursor { field.len() } else { b }
                    };
                    field.insert_str(byte_pos, &text);
                    *cursor += text.chars().count();
                }
            },
            Ok(_) => {}
            Err(e) => app.set_toast(format!("Paste failed: {e}"), crate::app::ToastLevel::Error),
        }
        return None;
    }

    match app.chat_panel.sub_mode {
        ChatSubMode::ModelSelect => return handle_model_select(app, key),
        ChatSubMode::Settings => return handle_settings(app, key),
        ChatSubMode::Chat => {}
    }

    // Any key other than a repeated Ctrl+L disarms the pending clear.
    if app.chat_panel.pending_clear
        && !(key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        app.chat_panel.pending_clear = false;
    }

    match key.code {
        KeyCode::Esc => {
            // Hide overlay — state is preserved
            app.mode = AppMode::Normal;
        }
        KeyCode::Enter
            if !key.modifiers.contains(KeyModifiers::SHIFT)
                && !app.chat_panel.streaming
                && !app.chat_panel.input.trim().is_empty() =>
        {
            return Some(Action::ChatSendMessage);
        }
        KeyCode::Tab => {
            // Open model selector
            if !app.chat_panel.models.is_empty() {
                app.chat_panel.sub_mode = ChatSubMode::ModelSelect;
                app.chat_panel.model_filter.clear();
                // Pre-select the current model and clamp
                if let Some(pos) = app
                    .chat_panel
                    .models
                    .iter()
                    .position(|m| *m == app.chat_panel.config.model)
                {
                    app.chat_panel.model_selected = pos;
                } else {
                    app.chat_panel.model_selected = app
                        .chat_panel
                        .model_selected
                        .min(app.chat_panel.models.len() - 1);
                }
                app.needs_redraw = true;
            } else {
                // Try to load models
                return Some(Action::ChatLoadModels);
            }
        }
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Open settings
            app.chat_panel.settings_server_type = app.chat_panel.config.server_type;
            app.chat_panel.settings_url = app.chat_panel.config.base_url.clone();
            app.chat_panel.settings_prompt = app
                .chat_panel
                .config
                .system_prompt
                .clone()
                .unwrap_or_default();
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
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Stop the streaming response; the partial text is kept.
            if app.chat_panel.streaming {
                app.chat_stop_stream();
                app.set_toast("Stopped streaming response", crate::app::ToastLevel::Info);
            }
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Two-step clear: first press arms (footer shows the prompt),
            // second press wipes the conversation and any in-flight stream.
            if app.chat_panel.messages.is_empty() && app.chat_panel.current_response.is_empty() {
                app.chat_panel.pending_clear = false;
            } else if app.chat_panel.pending_clear {
                app.chat_panel.pending_clear = false;
                if let Some(handle) = app.chat_panel.stream_abort.take() {
                    handle.abort();
                }
                app.chat_panel.streaming = false;
                app.chat_panel.agent_tool_status = None;
                app.chat_panel.last_stream_activity = None;
                app.chat_panel.messages.clear();
                app.chat_panel.current_response.clear();
                app.chat_panel.scroll = 0;
            } else {
                app.chat_panel.pending_clear = true;
            }
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
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Toggle web search for OpenRouter
            app.chat_panel.config.web_search = !app.chat_panel.config.web_search;
            // Persist to provider manager as well
            let provider_name = app.chat_panel.config.provider.clone();
            if let Some(entry) = app.chat_provider_manager.get_mut(&provider_name) {
                entry.web_search = app.chat_panel.config.web_search;
                let _ = app
                    .chat_provider_manager
                    .save(&app.paths.chat_providers_path());
            }
            save_chat_config(app);
            let label = if app.chat_panel.config.web_search {
                "Web search ON (OpenRouter plugins: web)"
            } else {
                "Web search OFF"
            };
            app.set_toast(label, crate::app::ToastLevel::Info);
        }
        KeyCode::Char(c) => {
            app.chat_panel.input.insert(app.chat_panel.input_cursor, c);
            app.chat_panel.input_cursor += c.len_utf8();
        }
        KeyCode::Backspace if app.chat_panel.input_cursor > 0 => {
            let prev = prev_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
            app.chat_panel
                .input
                .drain(prev..app.chat_panel.input_cursor);
            app.chat_panel.input_cursor = prev;
        }
        KeyCode::Delete if app.chat_panel.input_cursor < app.chat_panel.input.len() => {
            let next = next_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
            app.chat_panel
                .input
                .drain(app.chat_panel.input_cursor..next);
        }
        KeyCode::Left if app.chat_panel.input_cursor > 0 => {
            app.chat_panel.input_cursor =
                prev_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
        }
        KeyCode::Right if app.chat_panel.input_cursor < app.chat_panel.input.len() => {
            app.chat_panel.input_cursor =
                next_char_boundary(&app.chat_panel.input, app.chat_panel.input_cursor);
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

fn filtered_indices(app: &App) -> Vec<usize> {
    let filter = app.chat_panel.model_filter.trim().to_lowercase();
    if filter.is_empty() {
        return (0..app.chat_panel.models.len()).collect();
    }
    app.chat_panel
        .models
        .iter()
        .enumerate()
        .filter(|(_, m)| m.to_lowercase().contains(&filter))
        .map(|(i, _)| i)
        .collect()
}

fn handle_model_select(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Ctrl+U clears filter
    if key.code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.chat_panel.model_filter.clear();
        app.chat_panel.model_selected = 0;
        app.needs_redraw = true;
        return None;
    }
    let filtered = filtered_indices(app);
    let total = filtered.len();
    match key.code {
        KeyCode::Esc => {
            if !app.chat_panel.model_filter.is_empty() {
                app.chat_panel.model_filter.clear();
                app.chat_panel.model_selected = 0;
                app.needs_redraw = true;
            } else {
                app.chat_panel.sub_mode = ChatSubMode::Chat;
                app.needs_redraw = true;
            }
        }
        KeyCode::Tab => {
            app.chat_panel.sub_mode = ChatSubMode::Chat;
            app.needs_redraw = true;
        }
        KeyCode::Up if app.chat_panel.model_selected > 0 => {
            app.chat_panel.model_selected -= 1;
            app.needs_redraw = true;
        }
        KeyCode::Down if app.chat_panel.model_selected + 1 < total => {
            app.chat_panel.model_selected += 1;
            app.needs_redraw = true;
        }
        KeyCode::Backspace => {
            if !app.chat_panel.model_filter.is_empty() {
                app.chat_panel.model_filter.pop();
                // clamp selection to new filtered size
                let new_filtered = filtered_indices(app);
                if new_filtered.is_empty() {
                    app.chat_panel.model_selected = 0;
                } else if app.chat_panel.model_selected >= new_filtered.len() {
                    app.chat_panel.model_selected = new_filtered.len() - 1;
                }
                app.needs_redraw = true;
            }
        }
        KeyCode::Enter => {
            if let Some(&real_idx) = filtered.get(app.chat_panel.model_selected)
                && let Some(name) = app.chat_panel.models.get(real_idx).cloned()
            {
                app.chat_panel.config.model = name.clone();
                // Persist model to chat-providers.toml for this provider estilo provider
                let provider_name = app.chat_panel.config.provider.clone();
                if let Some(entry) = app.chat_provider_manager.get_mut(&provider_name) {
                    entry.model = name.clone();
                    let _ = app
                        .chat_provider_manager
                        .save(&app.paths.chat_providers_path());
                }
                save_chat_config(app);
            }
            app.chat_panel.model_filter.clear();
            app.chat_panel.sub_mode = ChatSubMode::Chat;
            app.needs_redraw = true;
        }
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            app.chat_panel.model_filter.push(c);
            app.chat_panel.model_selected = 0;
            app.needs_redraw = true;
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
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return save_and_close_settings(app);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Cycle provider (ChatServerType) and load its saved config from chat-providers.toml
                let old_type = app.chat_panel.settings_server_type;
                let new_type = old_type.next();
                app.chat_panel.settings_server_type = new_type;
                // Load provider's persisted base_url/system_prompt so switching doesn't require manual edit
                let provider_name = match new_type {
                    piki_core::chat::ChatServerType::Ollama => "ollama",
                    piki_core::chat::ChatServerType::LlamaCpp => "llama.cpp",
                    piki_core::chat::ChatServerType::OpenRouter => "openrouter",
                };
                if let Some(p) = app.chat_provider_manager.get(provider_name) {
                    app.chat_panel.settings_url = p.base_url.clone();
                    app.chat_panel.settings_prompt = p.system_prompt.clone().unwrap_or_default();
                    app.chat_panel.config.web_search = p.web_search;
                    // keep settings_url in sync; model is applied on save
                } else {
                    let old_default = old_type.default_url();
                    if app.chat_panel.settings_url == old_default {
                        app.chat_panel.settings_url = new_type.default_url().to_string();
                    }
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
                ChatSettingsField::ServerType => (
                    ChatSettingsField::BaseUrl,
                    app.chat_panel.settings_url.len(),
                ),
                ChatSettingsField::BaseUrl => (
                    ChatSettingsField::SystemPrompt,
                    app.chat_panel.settings_prompt.len(),
                ),
                ChatSettingsField::SystemPrompt => (ChatSettingsField::ServerType, 0),
            };
            app.chat_panel.settings_field = new_field;
            app.chat_panel.settings_cursor = new_cursor;
        }
        KeyCode::Up => {
            // Cycle backward: SystemPrompt -> BaseUrl -> ServerType -> ...
            let (new_field, new_cursor) = match app.chat_panel.settings_field {
                ChatSettingsField::ServerType => (
                    ChatSettingsField::SystemPrompt,
                    app.chat_panel.settings_prompt.len(),
                ),
                ChatSettingsField::BaseUrl => (ChatSettingsField::ServerType, 0),
                ChatSettingsField::SystemPrompt => (
                    ChatSettingsField::BaseUrl,
                    app.chat_panel.settings_url.len(),
                ),
            };
            app.chat_panel.settings_field = new_field;
            app.chat_panel.settings_cursor = new_cursor;
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
    let final_url = if url.is_empty() {
        new_server_type.default_url().to_string()
    } else {
        url.clone()
    };
    app.chat_panel.config.base_url = final_url.clone();
    app.chat_panel.config.system_prompt = if prompt.is_empty() {
        None
    } else {
        Some(prompt.clone())
    };
    // Keep provider name in sync (for chat-providers.toml estilo provider)
    app.chat_panel.config.provider = match new_server_type {
        piki_core::chat::ChatServerType::Ollama => "ollama".to_string(),
        piki_core::chat::ChatServerType::LlamaCpp => "llama.cpp".to_string(),
        piki_core::chat::ChatServerType::OpenRouter => "openrouter".to_string(),
    };

    if server_changed {
        // Apply provider's saved model and web_search when switching provider
        let provider_name = app.chat_panel.config.provider.as_str();
        if let Some(p) = app.chat_provider_manager.get(provider_name) {
            if !p.model.is_empty() {
                app.chat_panel.config.model = p.model.clone();
            } else {
                app.chat_panel.config.model.clear();
            }
            app.chat_panel.config.web_search = p.web_search;
        } else {
            app.chat_panel.config.model.clear();
            app.chat_panel.config.web_search = false;
        }
    }

    // Persist provider-specific config to chat-providers.toml estilo providers.toml
    let provider_name = app.chat_panel.config.provider.clone();
    let provider_model = app.chat_panel.config.model.clone();
    let provider_web_search = app.chat_panel.config.web_search;
    let provider_cfg = piki_core::chat_providers::ChatProviderConfig {
        name: provider_name.clone(),
        description: String::new(),
        server_type: new_server_type,
        base_url: final_url,
        model: provider_model,
        system_prompt: if prompt.is_empty() {
            None
        } else {
            Some(prompt)
        },
        web_search: provider_web_search,
    };
    app.chat_provider_manager.upsert(provider_cfg);
    let _ = app
        .chat_provider_manager
        .save(&app.paths.chat_providers_path());

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
pub(crate) fn active_field_mut(app: &mut App) -> (&mut String, &mut usize) {
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

fn save_chat_config(app: &mut App) {
    let result = match serde_json::to_string(&app.chat_panel.config) {
        Ok(json) => match app.storage.ui_prefs {
            Some(ref ui_prefs) => ui_prefs
                .set_preference("chat_config", &json)
                .map_err(|e| e.to_string()),
            None => Ok(()),
        },
        Err(e) => Err(e.to_string()),
    };
    if let Err(e) = result {
        app.set_toast(
            format!("Failed to save chat settings: {e}"),
            crate::app::ToastLevel::Error,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, ChatSubMode};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use piki_core::paths::DataPaths;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn test_model_select_navigation_persistence_and_redraw() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(dir.path().to_path_buf());
        let storage = std::sync::Arc::new(piki_core::storage::create_storage(&paths).unwrap());
        let mut app = App::new(storage, &paths);
        // Setup models like OpenRouter
        let models = vec![
            "openai/gpt-4o-mini".to_string(),
            "openai/gpt-4o".to_string(),
            "anthropic/claude-3.5-sonnet".to_string(),
            "google/gemini-flash-1.5".to_string(),
            "meta-llama/llama-3.1-70b".to_string(),
        ];
        app.chat_panel.models = models.clone();
        app.chat_panel.config.model = "openai/gpt-4o-mini".to_string();
        app.chat_panel.config.provider = "openrouter".to_string();
        app.chat_panel.sub_mode = ChatSubMode::ModelSelect;
        app.chat_panel.model_selected = 0;
        app.needs_redraw = false;

        // Test Down via ArrowDown (j/k now type into filter, not navigate)
        handle_model_select(&mut app, key(KeyCode::Down));
        assert_eq!(app.chat_panel.model_selected, 1, "Down should go 0->1");
        assert!(app.needs_redraw, "Down should set needs_redraw");
        app.needs_redraw = false;

        handle_model_select(&mut app, key(KeyCode::Down));
        assert_eq!(
            app.chat_panel.model_selected, 2,
            "Down Arrow should go 1->2"
        );
        assert!(app.needs_redraw);
        app.needs_redraw = false;

        // Test Up via ArrowUp
        handle_model_select(&mut app, key(KeyCode::Up));
        assert_eq!(app.chat_panel.model_selected, 1, "Up should go 2->1");
        assert!(app.needs_redraw);
        app.needs_redraw = false;

        handle_model_select(&mut app, key(KeyCode::Up));
        assert_eq!(app.chat_panel.model_selected, 0, "Up Arrow should go 1->0");
        assert!(app.needs_redraw);
        app.needs_redraw = false;

        // Test typing filters (j/k now append to filter, not navigate)
        handle_model_select(&mut app, key(KeyCode::Char('c')));
        assert_eq!(app.chat_panel.model_filter, "c");
        assert_eq!(app.chat_panel.model_selected, 0);
        handle_model_select(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.chat_panel.model_filter, "cj");
        // Backspace removes last char
        handle_model_select(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.chat_panel.model_filter, "c");
        handle_model_select(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.chat_panel.model_filter, "");
        app.needs_redraw = false;

        // Test Esc
        app.chat_panel.sub_mode = ChatSubMode::ModelSelect;
        handle_model_select(&mut app, key(KeyCode::Esc));
        assert_eq!(
            app.chat_panel.sub_mode,
            ChatSubMode::Chat,
            "Esc should close"
        );
        assert!(app.needs_redraw);
        app.needs_redraw = false;

        // Test Tab
        app.chat_panel.sub_mode = ChatSubMode::ModelSelect;
        handle_model_select(&mut app, key(KeyCode::Tab));
        assert_eq!(
            app.chat_panel.sub_mode,
            ChatSubMode::Chat,
            "Tab should close"
        );
        assert!(app.needs_redraw);
        app.needs_redraw = false;

        // Test Enter selects and persists to chat-providers.toml
        app.chat_panel.sub_mode = ChatSubMode::ModelSelect;
        app.chat_panel.model_selected = 2;
        let expected = "anthropic/claude-3.5-sonnet".to_string();
        handle_model_select(&mut app, key(KeyCode::Enter));
        assert_eq!(
            app.chat_panel.config.model, expected,
            "Enter should select model"
        );
        assert_eq!(app.chat_panel.sub_mode, ChatSubMode::Chat);
        assert!(app.needs_redraw);
        // Verify persistence
        let mgr2 = piki_core::chat_providers::ChatProviderManager::load_or_init(
            &app.paths.chat_providers_path(),
        );
        let persisted = mgr2.get("openrouter").unwrap();
        assert_eq!(
            persisted.model, expected,
            "should persist to chat-providers.toml"
        );

        // Test event loop model loading sets needs_redraw (simulated)
        let dir2 = tempfile::tempdir().unwrap();
        let paths2 = DataPaths::new(dir2.path().join("2"));
        let mut app2 = App::new(
            std::sync::Arc::new(piki_core::storage::create_storage(&paths2).unwrap()),
            &paths2,
        );
        app2.chat_panel.models = vec![];
        app2.chat_panel.config.model = "".to_string();
        app2.needs_redraw = false;
        let model_data = "openai/gpt-4o-mini\nopenai/gpt-4o\nanthropic/claude\n";
        app2.chat_panel.models = model_data
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        if app2.chat_panel.config.model.is_empty()
            && let Some(first) = app2.chat_panel.models.first()
        {
            app2.chat_panel.config.model = first.clone();
        }
        app2.needs_redraw = true;
        assert_eq!(app2.chat_panel.models.len(), 3);
        assert_eq!(app2.chat_panel.config.model, "openai/gpt-4o-mini");
        assert!(app2.needs_redraw);

        // Multi-frame responsiveness: 5 frames Tab, j, j, Enter, Tab
        let dir3 = tempfile::tempdir().unwrap();
        let paths3 = DataPaths::new(dir3.path().join("3"));
        let mut app3 = App::new(
            std::sync::Arc::new(piki_core::storage::create_storage(&paths3).unwrap()),
            &paths3,
        );
        app3.chat_panel.models = models.clone();
        app3.chat_panel.config.model = "openai/gpt-4o-mini".to_string();
        app3.chat_panel.config.provider = "openrouter".to_string();
        let mut frames = 0;
        // Frame 1: Tab open
        app3.chat_panel.sub_mode = ChatSubMode::ModelSelect;
        app3.chat_panel.model_selected = 0;
        app3.needs_redraw = true;
        frames += 1;
        assert!(app3.needs_redraw);
        // Frame 2: Down
        app3.needs_redraw = false;
        handle_model_select(&mut app3, key(KeyCode::Down));
        frames += 1;
        assert_eq!(app3.chat_panel.model_selected, 1);
        assert!(app3.needs_redraw);
        // Frame 3: Down
        app3.needs_redraw = false;
        handle_model_select(&mut app3, key(KeyCode::Down));
        frames += 1;
        assert_eq!(app3.chat_panel.model_selected, 2);
        // Frame 4: Enter
        app3.needs_redraw = false;
        handle_model_select(&mut app3, key(KeyCode::Enter));
        frames += 1;
        assert_eq!(app3.chat_panel.config.model, "anthropic/claude-3.5-sonnet");
        assert!(app3.needs_redraw);
        // Frame 5: Tab
        app3.needs_redraw = false;
        app3.chat_panel.sub_mode = ChatSubMode::ModelSelect;
        app3.needs_redraw = true;
        frames += 1;
        assert_eq!(frames, 5);
    }
}
