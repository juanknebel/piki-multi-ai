use std::path::PathBuf;

use ratatui::DefaultTerminal;

use crate::app::App;
use crate::dialog_state::ConflictStrategy;
use piki_core::workspace::WorkspaceManager;
use piki_core::{AIProvider, MergeStrategy, WorkspaceType};

mod agent;
mod api;
mod chat;
mod files;
mod git;
mod git_merge;
mod git_stash;
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
        Option<String>,
    ),
    /// Clone a GitHub URL into a user-chosen destination directory and
    /// register as Simple. Args:
    /// (name, description, prompt, kanban_path, github_url, destination_dir, group)
    CreateGithubWorkspace(
        String,
        String,
        String,
        Option<String>,
        String,
        std::path::PathBuf,
        Option<String>,
    ),
    EditWorkspace(usize, Option<String>, String, Option<String>),
    /// Second field: optional target kanban column for dispatched cards
    DeleteWorkspace(usize, Option<String>),
    /// Remove workspace from app list but keep worktree on disk
    RemoveFromList(usize),
    /// Open diff for the file at the given index in the active workspace
    OpenDiff(usize),
    /// Open $EDITOR for a file path
    OpenEditor(PathBuf),
    /// Git: stage a file at the given index
    GitStage(usize),
    /// Git: unstage a file at the given index
    GitUnstage(usize),
    /// Git: stage all multi-selected files
    GitStageSelected,
    /// Git: unstage all multi-selected files
    GitUnstageSelected,
    /// Git: commit with message
    GitCommit(String),
    /// Git: push current branch
    GitPush,
    /// Spawn a new tab with the given provider
    SpawnTab(AIProvider),
    /// Open a markdown file in a new tab
    OpenMarkdown(PathBuf),
    /// Open a markdown file in external mdr viewer
    OpenMdr(PathBuf),
    /// Git: merge workspace branch into main
    GitMerge(MergeStrategy),
    /// Undo last stage/unstage action
    Undo,
    /// Load PR review data (info + files) for the active workspace
    LoadPrReview,
    /// Load diff for a specific file in the PR review
    LoadPrFileDiff(usize),
    /// Submit the PR review using the draft state
    SubmitPrReview,
    /// Send an API request (raw Hurl text)
    SendApiRequest(String),
    /// Load git log for the active workspace
    LoadGitLog,
    /// View diff for a specific commit by SHA
    ViewCommitDiff(String),
    /// Git stash: list all stash entries
    GitStashList,
    /// Git stash: save with message
    GitStashSave(String),
    /// Git stash: pop entry at index
    GitStashPop(usize),
    /// Git stash: apply entry at index
    GitStashApply(usize),
    /// Git stash: drop entry at index
    GitStashDrop(usize),
    /// Git stash: show diff for entry at index
    GitStashShow(usize),
    /// View the conflict diff for a file (shows ours vs theirs)
    ViewConflictDiff(String),
    /// Resolve a merge conflict on a single file using the given strategy
    ResolveConflict {
        file: String,
        strategy: ConflictStrategy,
    },
    /// Abort the current merge or rebase
    AbortMerge,
    /// Scan for conflicts in worktree and source_repo, open resolution overlay
    DetectConflicts,
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
        | Action::RemoveFromList(..) => {
            workspace::handle(app, manager, action, terminal).await?
        }
        Action::OpenEditor(..) | Action::OpenDiff(..) => {
            files::handle(app, manager, action, terminal).await?
        }
        Action::GitStage(..)
        | Action::GitUnstage(..)
        | Action::GitStageSelected
        | Action::GitUnstageSelected
        | Action::GitCommit(..)
        | Action::GitPush
        | Action::Undo
        | Action::LoadGitLog
        | Action::ViewCommitDiff(..) => {
            git::handle(app, manager, action, terminal).await?
        }
        Action::GitMerge(..)
        | Action::ViewConflictDiff(..)
        | Action::ResolveConflict { .. }
        | Action::AbortMerge
        | Action::DetectConflicts => {
            git_merge::handle(app, manager, action, terminal).await?
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
        Action::GitStashList
        | Action::GitStashSave(..)
        | Action::GitStashPop(..)
        | Action::GitStashApply(..)
        | Action::GitStashDrop(..)
        | Action::GitStashShow(..) => {
            git_stash::handle(app, manager, action, terminal).await?
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

