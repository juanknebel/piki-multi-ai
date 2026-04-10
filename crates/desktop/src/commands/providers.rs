use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub description: String,
    pub command: String,
    pub default_args: Vec<String>,
    pub prompt_format: String,
    pub prompt_flag: String,
    pub dispatchable: bool,
    pub agent_dir: Option<String>,
}

fn config_to_info(config: &piki_core::providers::ProviderConfig) -> ProviderInfo {
    let (prompt_format, prompt_flag) = match &config.prompt_format {
        piki_core::providers::PromptFormat::Positional => ("Positional".into(), String::new()),
        piki_core::providers::PromptFormat::Flag(f) => ("Flag".into(), f.clone()),
        piki_core::providers::PromptFormat::None => ("None".into(), String::new()),
    };
    ProviderInfo {
        name: config.name.clone(),
        description: config.description.clone(),
        command: config.command.clone(),
        default_args: config.default_args.clone(),
        prompt_format,
        prompt_flag,
        dispatchable: config.dispatchable,
        agent_dir: config.agent_dir.clone(),
    }
}

#[tauri::command]
pub async fn list_providers(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Vec<ProviderInfo>, String> {
    let app = state.lock();
    Ok(app.provider_manager.all().iter().map(config_to_info).collect())
}

#[derive(Deserialize)]
pub struct SaveProviderArgs {
    pub name: String,
    pub description: String,
    pub command: String,
    pub default_args: Vec<String>,
    pub prompt_format: String,
    pub prompt_flag: String,
    pub dispatchable: bool,
    pub agent_dir: Option<String>,
}

#[tauri::command]
pub async fn save_provider(
    state: State<'_, Mutex<DesktopApp>>,
    provider: SaveProviderArgs,
) -> Result<(), String> {
    let prompt_format = match provider.prompt_format.as_str() {
        "Flag" => piki_core::providers::PromptFormat::Flag(provider.prompt_flag),
        "None" => piki_core::providers::PromptFormat::None,
        _ => piki_core::providers::PromptFormat::Positional,
    };
    let config = piki_core::providers::ProviderConfig {
        name: provider.name,
        description: provider.description,
        command: provider.command,
        default_args: provider.default_args,
        prompt_format,
        dispatchable: provider.dispatchable,
        agent_dir: provider.agent_dir.filter(|s| !s.is_empty()),
    };
    let mut app = state.lock();
    app.provider_manager.upsert(config);
    app.provider_manager
        .save(&app.paths.providers_path())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_provider(
    state: State<'_, Mutex<DesktopApp>>,
    name: String,
) -> Result<bool, String> {
    let mut app = state.lock();
    let removed = app.provider_manager.remove(&name);
    if removed {
        app.provider_manager
            .save(&app.paths.providers_path())
            .map_err(|e| e.to_string())?;
    }
    Ok(removed)
}
