use std::path::PathBuf;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct KanbanCard {
    pub id: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub assignee: String,
    pub project: String,
}

#[derive(Serialize, Clone)]
pub struct KanbanColumn {
    pub id: String,
    pub cards: Vec<KanbanCard>,
}

#[derive(Serialize, Clone)]
pub struct KanbanBoard {
    pub columns: Vec<KanbanColumn>,
}

fn resolve_board_path(app: &DesktopApp, workspace_idx: usize) -> Result<PathBuf, String> {
    let ws = app
        .workspaces
        .get(workspace_idx)
        .ok_or("Workspace index out of range")?;
    ws.info
        .kanban_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "No kanban board configured for this workspace".to_string())
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    if let Some(path_str) = path.to_str() {
        if path_str.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(path_str.strip_prefix("~/").unwrap());
            }
        }
    }
    path
}

fn ensure_board_exists(path: &PathBuf) -> Result<(), String> {
    let board_txt = path.join("board.txt");
    if !board_txt.exists() {
        std::fs::create_dir_all(path).map_err(|e| format!("Failed to create kanban dir: {e}"))?;
        let content =
            "col todo \"TO DO\"\ncol in_progress \"IN PROGRESS\"\ncol in_review \"IN REVIEW\"\ncol done \"DONE\"\n";
        std::fs::write(&board_txt, content)
            .map_err(|e| format!("Failed to write board.txt: {e}"))?;
        for col in &["todo", "in_progress", "in_review", "done"] {
            let col_dir = path.join("cols").join(col);
            let _ = std::fs::create_dir_all(&col_dir);
            let _ = std::fs::write(col_dir.join("order.txt"), "");
        }
    }
    Ok(())
}

fn create_provider(
    path: PathBuf,
) -> Result<Box<dyn flow_core::provider::Provider>, String> {
    let expanded = expand_tilde(path);
    ensure_board_exists(&expanded)?;
    Ok(Box::new(
        flow_core::provider_local::LocalProvider::new(expanded),
    ))
}

fn map_card(card: &flow_core::Card) -> KanbanCard {
    let priority = match card.priority {
        flow_core::Priority::Bug => "Bug",
        flow_core::Priority::High => "High",
        flow_core::Priority::Medium => "Medium",
        flow_core::Priority::Low => "Low",
        flow_core::Priority::Wishlist => "Wishlist",
    };
    KanbanCard {
        id: card.id.clone(),
        title: card.title.clone(),
        description: card.description.clone(),
        priority: priority.to_string(),
        assignee: card.assignee.clone(),
        project: card.project.clone(),
    }
}

fn map_board(board: &flow_core::Board) -> KanbanBoard {
    KanbanBoard {
        columns: board
            .columns
            .iter()
            .map(|col| KanbanColumn {
                id: col.id.clone(),
                cards: col.cards.iter().map(map_card).collect(),
            })
            .collect(),
    }
}

fn parse_priority(s: &str) -> flow_core::Priority {
    match s.to_uppercase().as_str() {
        "BUG" => flow_core::Priority::Bug,
        "HIGH" => flow_core::Priority::High,
        "MEDIUM" => flow_core::Priority::Medium,
        "LOW" => flow_core::Priority::Low,
        "WISHLIST" => flow_core::Priority::Wishlist,
        _ => flow_core::Priority::Medium,
    }
}

#[tauri::command]
pub fn kanban_load_board(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    sort: Option<String>,
    project_filter: Option<Vec<String>>,
) -> Result<KanbanBoard, String> {
    let board_path = {
        let app = state.lock();
        resolve_board_path(&app, workspace_idx)?
    };

    let mut provider = create_provider(board_path)?;
    let mut board = provider
        .load_board()
        .map_err(|e| format!("Failed to load board: {e}"))?;

    if let Some(ref pf) = project_filter {
        board.apply_project_filter(pf);
    }

    match sort.as_deref() {
        Some("asc") => board.sort_cards_with(flow_core::SortOrder::Asc),
        Some("desc") => board.sort_cards_with(flow_core::SortOrder::Desc),
        _ => {}
    }

    Ok(map_board(&board))
}

#[tauri::command]
pub fn kanban_create_card(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    column_id: String,
    project: String,
) -> Result<String, String> {
    let board_path = {
        let app = state.lock();
        resolve_board_path(&app, workspace_idx)?
    };

    let mut provider = create_provider(board_path)?;
    let card_id = provider
        .create_card(&column_id, &project)
        .map_err(|e| format!("Failed to create card: {e}"))?;
    Ok(card_id)
}

#[tauri::command]
pub fn kanban_update_card(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    card_id: String,
    title: String,
    description: String,
    priority: String,
    assignee: String,
    project: String,
) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if project.trim().is_empty() {
        return Err("Project is required".to_string());
    }
    let board_path = {
        let app = state.lock();
        resolve_board_path(&app, workspace_idx)?
    };

    let mut provider = create_provider(board_path)?;
    let prio = parse_priority(&priority);
    provider
        .update_card(&card_id, &title, &description, prio, &assignee, &project)
        .map_err(|e| format!("Failed to update card: {e}"))
}

#[tauri::command]
pub fn kanban_move_card(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    card_id: String,
    to_column_id: String,
) -> Result<(), String> {
    let board_path = {
        let app = state.lock();
        resolve_board_path(&app, workspace_idx)?
    };

    let mut provider = create_provider(board_path)?;
    provider
        .move_card(&card_id, &to_column_id)
        .map_err(|e| format!("Failed to move card: {e}"))
}

#[tauri::command]
pub fn kanban_delete_card(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    card_id: String,
) -> Result<(), String> {
    let board_path = {
        let app = state.lock();
        resolve_board_path(&app, workspace_idx)?
    };

    let mut provider = create_provider(board_path)?;
    provider
        .delete_card(&card_id)
        .map_err(|e| format!("Failed to delete card: {e}"))
}

#[tauri::command]
pub fn kanban_load_board_by_path(board_path: String) -> Result<KanbanBoard, String> {
    let mut provider = create_provider(PathBuf::from(board_path))?;
    let board = provider
        .load_board()
        .map_err(|e| format!("Failed to load board: {e}"))?;
    Ok(map_board(&board))
}

#[tauri::command]
pub fn kanban_move_card_by_path(
    board_path: String,
    card_id: String,
    to_column_id: String,
) -> Result<(), String> {
    let mut provider = create_provider(PathBuf::from(board_path))?;
    provider
        .move_card(&card_id, &to_column_id)
        .map_err(|e| format!("Failed to move card: {e}"))?;
    Ok(())
}
