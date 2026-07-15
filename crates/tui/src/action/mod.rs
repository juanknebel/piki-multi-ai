use std::path::PathBuf;

use ratatui::DefaultTerminal;

use crate::app::App;
use piki_core::workspace::WorkspaceManager;
use piki_core::{AIProvider, WorkspaceType};

mod agent;
mod api;
mod chat;
mod files;
mod review;
mod tabs;
mod workspace;

/// Async actions triggered by key events
#[derive(Debug)]
pub(crate) enum Action {
    CreateWorkspace(
        String,
        String,
        String,
        Option<String>,
        PathBuf,
        WorkspaceType,
    ),
    /// Clone a GitHub URL into a user-chosen destination directory and
    /// register as Simple. Args:
    /// (name, description, prompt, kanban_path, github_url, destination_dir)
    CreateGithubWorkspace(
        String,
        String,
        String,
        Option<String>,
        String,
        std::path::PathBuf,
    ),
    EditWorkspace(usize, Option<String>, String),
    /// Second field: optional target kanban column for dispatched cards
    DeleteWorkspace(usize, Option<String>),
    /// Scan disk (via `git worktree list`) for worktrees of the parent
    /// workspace's repo that aren't already registered, for the
    /// CreateWorktree dialog's "Load existing worktree" flow.
    ListWorktrees(usize),
    /// Register an already-existing worktree directory as a new workspace.
    ImportExistingWorktree {
        parent_idx: usize,
        path: PathBuf,
        branch: String,
    },
    /// Remove workspace from app list but keep worktree on disk
    RemoveFromList(usize),
    /// Open $EDITOR for a file path
    OpenEditor(PathBuf),
    /// Spawn a new tab with the given provider
    SpawnTab(AIProvider),
    /// Open a markdown file in a new tab
    OpenMarkdown(PathBuf),
    /// Open a markdown file in external mdr viewer
    OpenMdr(PathBuf),
    /// Load PR review data (info + files) for the active workspace
    LoadPrReview,
    /// Load diff for a specific file in the PR review
    LoadPrFileDiff(usize),
    /// Submit the PR review using the draft state
    SubmitPrReview,
    /// Send an API request (raw Hurl text)
    SendApiRequest(String),
    /// Dispatch an agent to work on a kanban card
    DispatchAgent {
        source_ws: usize,
        card_id: String,
        card_title: String,
        card_description: String,
        card_priority: flow_core::Priority,
        card_project: String,
        provider: AIProvider,
        agent_name: Option<String>,
        agent_role: Option<String>,
        additional_prompt: String,
        use_current_ws: bool,
    },
    /// Save an agent profile to storage
    SaveAgent {
        source_repo: std::path::PathBuf,
        profile: piki_core::storage::AgentProfile,
    },
    /// Delete an agent profile by ID
    DeleteAgent(i64),
    /// Persist agent config file to the repo (Simple workspace only)
    SyncAgentToRepo(i64),
    /// Scan repo for agent files and open import dialog
    ScanRepoAgents,
    /// Import selected agents from repo files into storage: Vec<(name, provider_label, role)>
    ImportAgents(Vec<(String, String, String)>),
    /// Send the current chat input to Ollama and stream the response
    ChatSendMessage,
    /// Load available Ollama models into chat_panel.models
    ChatLoadModels,
}

pub(crate) async fn execute_action(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(..)
        | Action::CreateGithubWorkspace(..)
        | Action::EditWorkspace(..)
        | Action::DeleteWorkspace(..)
        | Action::RemoveFromList(..)
        | Action::ListWorktrees(..)
        | Action::ImportExistingWorktree { .. } => {
            workspace::handle(app, manager, action, terminal).await?
        }
        Action::OpenEditor(..) => {
            files::handle(app, manager, action, terminal).await?
        }
        Action::LoadPrReview | Action::LoadPrFileDiff(..) | Action::SubmitPrReview => {
            review::handle(app, manager, action, terminal).await?
        }
        Action::SpawnTab(..) | Action::OpenMarkdown(..) | Action::OpenMdr(..) => {
            tabs::handle(app, manager, action, terminal).await?
        }
        Action::SendApiRequest(..) => {
            api::handle(app, manager, action, terminal).await?
        }
        Action::DispatchAgent { .. }
        | Action::SaveAgent { .. }
        | Action::DeleteAgent(..)
        | Action::SyncAgentToRepo(..)
        | Action::ScanRepoAgents
        | Action::ImportAgents(..) => {
            agent::handle(app, manager, action, terminal).await?
        }
        Action::ChatSendMessage | Action::ChatLoadModels => {
            chat::handle(app, manager, action, terminal).await?
        }
    }
    Ok(())
}

