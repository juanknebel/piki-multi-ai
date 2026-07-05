use std::path::PathBuf;
use std::sync::Arc;

use ratatui::DefaultTerminal;

use crate::app::{self, App, AppMode, ToastLevel};
use crate::code_review::CodeReviewState;
use crate::dialog_state::{ConflictStrategy, DialogState};
use crate::helpers::spawn_tab;
use piki_core::workspace::{FileWatcher, WorkspaceManager};
use piki_core::{AIProvider, MergeStrategy, WorkspaceType};

mod files;
mod git_stash;
mod git;
mod git_merge;
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
        Action::LoadPrReview => {
            let worktree_path = app
                .workspaces
                .get(app.active_workspace)
                .map(|ws| ws.path.clone());
            if let Some(worktree_path) = worktree_path {
                match piki_core::github::get_pr_for_branch(&worktree_path).await {
                    Ok(Some(pr_info)) => {
                        match piki_core::github::get_pr_files(&worktree_path).await {
                            Ok(files) => {
                                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                                    ws.code_review = Some(CodeReviewState::new(pr_info, files));
                                }
                                app.set_toast("PR loaded", ToastLevel::Success);
                            }
                            Err(e) => {
                                app.set_toast(
                                    format!("Failed to load PR files: {}", e),
                                    ToastLevel::Error,
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                        {
                            ws.close_tab(ws.active_tab);
                        }
                        app.set_toast("No open PR for this branch", ToastLevel::Error);
                    }
                    Err(e) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                        {
                            ws.close_tab(ws.active_tab);
                        }
                        app.set_toast(format!("gh error: {}", e), ToastLevel::Error);
                    }
                }
            }
        }
        Action::LoadPrFileDiff(file_idx) => {
            // Extract what we need before the async call
            let diff_data = app.workspaces.get_mut(app.active_workspace).and_then(|ws| {
                let cr = ws.code_review.as_mut()?;
                let file = cr.files.get(file_idx)?;
                let file_path = file.path.clone();
                if cr.file_diffs.contains_key(&file_path) {
                    return None; // Already cached
                }
                cr.loading = true;
                let base_ref = cr.pr_info.base_ref_name.clone();
                Some((ws.path.clone(), file_path, base_ref))
            });
            if let Some((worktree_path, file_path, base_ref)) = diff_data {
                match piki_core::github::get_pr_file_diff_raw(&worktree_path, &file_path, &base_ref)
                    .await
                {
                    Ok(parsed) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(ref mut cr) = ws.code_review
                        {
                            cr.file_diffs.insert(file_path, parsed);
                            cr.diff_scroll = 0;
                            cr.cursor_line = 0;
                            cr.loading = false;
                        }
                    }
                    Err(e) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(ref mut cr) = ws.code_review
                        {
                            cr.loading = false;
                        }
                        app.set_toast(format!("Diff error: {}", e), ToastLevel::Error);
                    }
                }
            }
        }
        Action::SubmitPrReview => {
            let submit_data = if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if let Some(cr) = ws.code_review.as_mut() {
                    let verdict = cr.draft.verdict;
                    let body = cr.draft.body.clone();
                    let comments = cr.draft.comments.clone();
                    let pr_number = cr.pr_info.number;
                    cr.show_submit = false;
                    Some((ws.info.path.clone(), verdict, body, comments, pr_number))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((worktree_path, verdict, body, comments, pr_number)) = submit_data {
                let result = if comments.is_empty() {
                    piki_core::github::submit_review(&worktree_path, verdict, &body).await
                } else {
                    piki_core::github::submit_review_with_comments(
                        &worktree_path,
                        pr_number,
                        verdict,
                        &body,
                        &comments,
                    )
                    .await
                };
                match result {
                    Ok(_) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                            ws.code_review = None;
                            if ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                        }
                        app.mode = AppMode::Normal;
                        app.interacting = false;
                        app.active_dialog = None;
                        app.set_toast(
                            format!("Review submitted: {}", verdict.label()),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let user_msg = if msg.contains("Can not request changes on your own")
                            || msg.contains("Can not approve your own")
                        {
                            "Cannot approve/request-changes on your own PR — use Comment"
                                .to_string()
                        } else if msg.contains("Unprocessable Entity") {
                            format!("GitHub rejected: {}", msg)
                        } else {
                            format!("Submit failed: {}", msg)
                        };
                        // Show error inside the submit overlay so user can see it and retry
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(cr) = ws.code_review.as_mut()
                        {
                            cr.show_submit = true;
                            cr.submit_error = Some(user_msg);
                        }
                    }
                }
            }
        }
        Action::SpawnTab(provider) => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if provider == AIProvider::Kanban && ws.kanban_app.is_none() {
                    // Initialize Kanban app if it doesn't exist yet for this workspace
                    let kanban_path_opt = ws.kanban_path.clone();

                    let mut kanban_provider = if app.config.kanban.provider == "jira" {
                        Box::new(flow_core::provider_jira::JiraProvider::from_env())
                            as Box<dyn flow_core::provider::Provider>
                    } else {
                        let default_path = kanban_path_opt
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                app.config
                                    .kanban
                                    .path
                                    .clone()
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_else(|| {
                                        piki_core::xdg::home_dir()
                                            .join(".config/flow/boards/default")
                                    })
                            });

                        let expanded_path = if let Some(rest) = default_path
                            .to_str()
                            .and_then(|path_str| path_str.strip_prefix("~/"))
                        {
                            if let Ok(home) = std::env::var("HOME") {
                                std::path::PathBuf::from(home).join(rest)
                            } else {
                                default_path
                            }
                        } else {
                            default_path
                        };

                        // Initialize if board.txt doesn't exist
                        let board_txt = expanded_path.join("board.txt");
                        if !board_txt.exists() {
                            if let Err(e) = std::fs::create_dir_all(&expanded_path) {
                                app.status_message =
                                    Some(format!("Failed to create kanban dir: {}", e));
                            } else {
                                let board_content = "col todo \"TO DO\"\ncol in_progress \"IN PROGRESS\"\ncol in_review \"IN REVIEW\"\ncol done \"DONE\"\n";
                                if let Err(e) = std::fs::write(&board_txt, board_content) {
                                    app.status_message =
                                        Some(format!("Failed to write board.txt: {}", e));
                                } else {
                                    for col in &["todo", "in_progress", "in_review", "done"] {
                                        let col_dir = expanded_path.join("cols").join(col);
                                        let _ = std::fs::create_dir_all(&col_dir);
                                        let _ = std::fs::write(col_dir.join("order.txt"), "");
                                    }
                                }
                            }
                        }

                        Box::new(flow_core::provider_local::LocalProvider::new(expanded_path))
                            as Box<dyn flow_core::provider::Provider>
                    };

                    let board = kanban_provider
                        .load_board()
                        .unwrap_or_else(|_e| flow_core::Board { columns: vec![] });
                    let mut kanban = flow_tui::App::new(board);
                    if kanban.board.columns.is_empty() {
                        kanban.banner =
                            Some("Load failed or empty board. Check board.txt.".to_string());
                    }
                    ws.kanban_app = Some(kanban);
                    ws.kanban_provider = Some(kanban_provider);
                }

                // Singleton guard: Kanban and Api tabs must not be duplicated
                if matches!(provider, AIProvider::Kanban | AIProvider::Api)
                    && let Some(idx) = ws.tabs.iter().position(|t| t.provider == provider)
                {
                    ws.active_tab = idx;
                    return Ok(());
                }

                let idx = spawn_tab(ws, &provider, app.pty_rows, app.pty_cols, None, Some(&app.provider_manager), &app.paths).await;
                ws.active_tab = idx;
                app.status_message = Some(format!("Opened {} tab", provider.label()));
            }

            // Code Review: check gh availability (lazy, cached) then load PR data
            if provider == AIProvider::CodeReview {
                // Lazy gh CLI check — run once, cache forever
                if app.gh_available.is_none() {
                    let gh_ok = tokio::process::Command::new("gh")
                        .arg("--version")
                        .output()
                        .await
                        .is_ok_and(|o| o.status.success());
                    let auth_ok = if gh_ok {
                        tokio::process::Command::new("gh")
                            .args(["auth", "status"])
                            .output()
                            .await
                            .is_ok_and(|o| o.status.success())
                    } else {
                        false
                    };
                    app.gh_available = Some(gh_ok && auth_ok);
                    if !gh_ok {
                        app.set_toast(
                            "gh CLI not found — install from https://cli.github.com/",
                            ToastLevel::Error,
                        );
                    } else if !auth_ok {
                        app.set_toast(
                            "gh not authenticated — run `gh auth login`",
                            ToastLevel::Error,
                        );
                    }
                }
                if app.gh_available != Some(true) {
                    // Remove the tab we just created
                    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                        && ws
                            .current_tab()
                            .is_some_and(|t| t.provider == AIProvider::CodeReview)
                    {
                        ws.close_tab(ws.active_tab);
                    }
                    return Ok(());
                }
                let worktree_path = app
                    .workspaces
                    .get(app.active_workspace)
                    .map(|ws| ws.info.path.clone());
                if let Some(worktree_path) = worktree_path {
                    match piki_core::github::get_pr_for_branch(&worktree_path).await {
                        Ok(Some(pr_info)) => {
                            match piki_core::github::get_pr_files(&worktree_path).await {
                                Ok(files) => {
                                    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                                        ws.code_review = Some(CodeReviewState::new(pr_info, files));
                                    }
                                    app.set_toast("PR loaded", ToastLevel::Success);
                                }
                                Err(e) => {
                                    app.set_toast(
                                        format!("Failed to load PR files: {}", e),
                                        ToastLevel::Error,
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                                && ws
                                    .current_tab()
                                    .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                            app.set_toast("No open PR for this branch", ToastLevel::Error);
                        }
                        Err(e) => {
                            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                                && ws
                                    .current_tab()
                                    .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                            app.set_toast(format!("gh error: {}", e), ToastLevel::Error);
                        }
                    }
                }
            }
        }
        Action::OpenMarkdown(path) => match std::fs::read_to_string(&path) {
            Ok(content) => {
                let label = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "markdown".to_string());
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.add_markdown_tab(label.clone(), content, Some(&app.syntax));
                    app.status_message = Some(format!("Opened {}", label));
                }
            }
            Err(e) => {
                app.status_message = Some(format!("Failed to read file: {}", e));
            }
        },
        Action::OpenMdr(path) => {
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            )?;
            ratatui::restore();
            let status = std::process::Command::new("mdr").arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture,
                crossterm::event::EnableBracketedPaste,
                crossterm::event::PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            )?;
            match status {
                Ok(s) if s.success() => {
                    app.status_message = Some(format!("mdr: {}", path.display()));
                }
                Ok(s) => {
                    app.status_message = Some(format!("mdr exited with: {}", s));
                }
                Err(_) => {
                    app.status_message =
                        Some("mdr not found. Install: cargo install markdown-reader".to_string());
                }
            }
            if app.mode == AppMode::FuzzySearch {
                app.fuzzy = None;
                app.mode = AppMode::Normal;
            }
        }
        Action::SendApiRequest(text) => {
            // Parse the Hurl text (supports multiple requests)
            let parsed_requests = match piki_api_client::parse_hurl_multi(&text) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "API Explorer: failed to parse request");
                    app.set_toast(format!("Parse error: {}", e), ToastLevel::Error);
                    return Ok(());
                }
            };

            // Set loading state
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(tab) = ws.current_tab_mut()
                && let Some(ref mut api) = tab.api_state
            {
                api.loading = true;
                api.responses.clear();
                let slot = Arc::clone(&api.pending_responses);
                let storage = Arc::clone(&app.storage);
                let source_repo = ws.source_repo.to_string_lossy().to_string();

                tokio::spawn(async move {
                    let mut results = Vec::with_capacity(parsed_requests.len());

                    for parsed in parsed_requests {
                        let url = if !parsed.url.contains("://") {
                            format!("https://{}", parsed.url)
                        } else {
                            parsed.url.clone()
                        };

                        let method_str = match parsed.method {
                            piki_api_client::Method::Get => "GET",
                            piki_api_client::Method::Post => "POST",
                            piki_api_client::Method::Put => "PUT",
                            piki_api_client::Method::Delete => "DELETE",
                            piki_api_client::Method::Patch => "PATCH",
                        };

                        let mut request = match parsed.method {
                            piki_api_client::Method::Get => piki_api_client::ApiRequest::get(""),
                            piki_api_client::Method::Post => piki_api_client::ApiRequest::post(""),
                            piki_api_client::Method::Put => piki_api_client::ApiRequest::put(""),
                            piki_api_client::Method::Delete => {
                                piki_api_client::ApiRequest::delete("")
                            }
                            piki_api_client::Method::Patch => {
                                piki_api_client::ApiRequest::patch("")
                            }
                        };
                        request.body = parsed.body.clone();
                        for (k, v) in &parsed.headers {
                            request.headers.insert(k.clone(), v.clone());
                        }

                        // Build request text for history
                        let request_text = {
                            let mut text = format!("{} {}", method_str, url);
                            for (k, v) in &parsed.headers {
                                text.push_str(&format!("\n{}: {}", k, v));
                            }
                            if let Some(ref body) = parsed.body {
                                let body_str = String::from_utf8_lossy(body);
                                text.push_str(&format!("\n\n{}", body_str));
                            }
                            text
                        };

                        let config = piki_api_client::ClientConfig::new(&url);
                        let client = match piki_api_client::HttpClient::new(config) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::error!(error = %e, "API Explorer: failed to create HTTP client");
                                results.push(app::ApiResponseDisplay {
                                    status: 0,
                                    elapsed_ms: 0,
                                    body: format!("Client error: {}", e),
                                    headers: String::new(),
                                });
                                continue;
                            }
                        };

                        let start = std::time::Instant::now();
                        let result =
                            <piki_api_client::HttpClient as piki_api_client::ApiClient>::execute(
                                &client, request,
                            )
                            .await;
                        let elapsed = start.elapsed().as_millis();

                        let display = match result {
                            Ok(resp) => {
                                let body_text = String::from_utf8_lossy(&resp.body).to_string();
                                let body = if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&body_text)
                                {
                                    serde_json::to_string_pretty(&json).unwrap_or(body_text)
                                } else {
                                    body_text
                                };
                                let headers = resp
                                    .headers
                                    .iter()
                                    .map(|(k, v)| format!("{}: {}", k, v))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                tracing::info!(status = resp.status, elapsed_ms = elapsed, url = %url, "API Explorer: request completed");
                                app::ApiResponseDisplay {
                                    status: resp.status,
                                    elapsed_ms: elapsed,
                                    body,
                                    headers,
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, url = %url, "API Explorer: request failed");
                                app::ApiResponseDisplay {
                                    status: 0,
                                    elapsed_ms: elapsed,
                                    body: format!("Error: {}", e),
                                    headers: String::new(),
                                }
                            }
                        };

                        // Persist to API history storage if available
                        if let Some(ref api_storage) = storage.api_history {
                            let entry = piki_core::storage::ApiHistoryEntry {
                                id: None,
                                source_repo: source_repo.clone(),
                                created_at: String::new(),
                                request_text,
                                method: method_str.to_string(),
                                url: url.clone(),
                                status: display.status,
                                elapsed_ms: display.elapsed_ms,
                                response_body: display.body.clone(),
                                response_headers: display.headers.clone(),
                            };
                            if let Err(e) = api_storage.save_api_entry(&entry) {
                                tracing::warn!(error = %e, "Failed to persist API history entry");
                            }
                        }

                        results.push(display);
                    }

                    let mut guard = slot.lock();
                    *guard = Some(results);
                });
            }
        }
        Action::GitStashList
        | Action::GitStashSave(..)
        | Action::GitStashPop(..)
        | Action::GitStashApply(..)
        | Action::GitStashDrop(..)
        | Action::GitStashShow(..) => {
            git_stash::handle(app, manager, action, terminal).await?
        }
        Action::DispatchAgent {
            source_ws,
            card_id,
            card_title,
            card_description,
            card_priority,
            card_project,
            provider,
            agent_name,
            agent_role,
            additional_prompt,
            use_current_ws,
        } => {
            // 1. Extract source workspace data
            let (source_dir, source_ws_name) = match app.workspaces.get(source_ws) {
                Some(ws) => (ws.source_repo.clone(), ws.name.clone()),
                None => {
                    app.status_message = Some("Source workspace not found".into());
                    return Ok(());
                }
            };
            let kanban_path = app
                .workspaces
                .get(source_ws)
                .and_then(|ws| ws.kanban_path.clone());

            // 2. Compose task prompt: always include card title so the agent starts working
            let task_prompt = if let Some(ref name) = agent_name {
                let mut parts = vec![format!(
                    "Use the {} agent to plan and then implement the task: {}",
                    name, card_title
                )];
                if !card_description.is_empty() {
                    parts.push(card_description.clone());
                }
                if !additional_prompt.trim().is_empty() {
                    parts.push(additional_prompt.trim().to_string());
                }
                parts.join("\n\n")
            } else {
                let mut parts = vec![card_title.clone()];
                if !card_description.is_empty() {
                    parts.push(card_description.clone());
                }
                if !additional_prompt.trim().is_empty() {
                    parts.push(additional_prompt.trim().to_string());
                }
                parts.join("\n\n")
            };

            if use_current_ws {
                // Use current workspace — just spawn a new tab, no worktree
                // Update kanban card
                let assignee_label = agent_name.as_deref().unwrap_or(provider.label());
                if let Some(src_ws) = app.workspaces.get_mut(source_ws)
                    && let Some(ref mut kp) = src_ws.kanban_provider
                {
                    let _ = kp.update_card(
                        &card_id,
                        &card_title,
                        &card_description,
                        card_priority,
                        assignee_label,
                        &card_project,
                    );
                    let _ = kp.move_card(&card_id, "in_progress");
                    if let Some(ref mut ka) = src_ws.kanban_app
                        && let Ok(board) = kp.load_board()
                    {
                        ka.board = board;
                        ka.clamp();
                    }
                }

                // Spawn tab in current workspace
                let ws = &mut app.workspaces[source_ws];
                let idx =
                    spawn_tab(ws, &provider, app.pty_rows, app.pty_cols, Some(&task_prompt), Some(&app.provider_manager), &app.paths).await;
                ws.active_tab = idx;

                app.set_toast(
                    format!("Task started: {} via {}", card_title, provider.label()),
                    ToastLevel::Success,
                );
            } else {
                // Create new worktree workspace (original flow)
                // Build branch name: <type>/<sanitized_card_id>
                let type_prefix = match card_priority {
                    flow_core::Priority::Bug => "bug",
                    flow_core::Priority::Wishlist => "spike",
                    _ => "feature",
                };
                let sanitized_id: String = card_id
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                let ws_name = format!("{}/{}", type_prefix, sanitized_id);
                let group_name = format!("{}-AGENTS", source_ws_name);

                let result = manager
                    .create(
                        &ws_name,
                        &card_title,
                        &task_prompt,
                        kanban_path.clone(),
                        &source_dir,
                    )
                    .await;

                match result {
                    Ok(mut info) => {
                        info.group = Some(group_name);
                        info.dispatch_card_id = Some(card_id.clone());
                        info.dispatch_source_kanban = kanban_path;
                        info.dispatch_agent_name = agent_name.clone();
                        info.order = app
                            .workspaces
                            .iter()
                            .map(|w| w.info.order)
                            .max()
                            .map(|m| m + 1)
                            .unwrap_or(0);

                        // Materialize agent config files in worktree
                        if let (Some(name), Some(role)) = (&agent_name, &agent_role) {
                            let _ = materialize_agent_config(&info.path, name, &provider, role, Some(&app.provider_manager));
                        }

                        // Update kanban card: set assignee and move to IN PROGRESS
                        let assignee_label =
                            agent_name.as_deref().unwrap_or(provider.label());
                        if let Some(src_ws) = app.workspaces.get_mut(source_ws)
                            && let Some(ref mut kp) = src_ws.kanban_provider
                        {
                            let _ = kp.update_card(
                                &card_id,
                                &card_title,
                                &card_description,
                                card_priority,
                                assignee_label,
                                &card_project,
                            );
                            let _ = kp.move_card(&card_id, "in_progress");
                            if let Some(ref mut ka) = src_ws.kanban_app
                                && let Ok(board) = kp.load_board()
                            {
                                ka.board = board;
                                ka.clamp();
                            }
                        }

                        // Create workspace and switch to it
                        app.workspaces.push(app::Workspace::from_info(info));
                        let new_idx = app.workspaces.len() - 1;
                        app.switch_workspace(new_idx);

                        // Start file watcher
                        let ws = &mut app.workspaces[new_idx];
                        match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
                            Ok(watcher) => ws.watcher = Some(watcher),
                            Err(e) => {
                                app.status_message = Some(format!("Watcher error: {}", e));
                            }
                        }

                        // Spawn AI provider tab with task prompt
                        let ws = &mut app.workspaces[new_idx];
                        let idx = spawn_tab(
                            ws,
                            &provider,
                            app.pty_rows,
                            app.pty_cols,
                            Some(&task_prompt),
                            Some(&app.provider_manager),
                            &app.paths,
                        )
                        .await;
                        ws.active_tab = idx;

                        // Persist config async
                        {
                            let source = app.workspaces[new_idx].source_repo.clone();
                            let infos: Vec<_> =
                                app.workspaces.iter().map(|w| w.info.clone()).collect();
                            let storage = Arc::clone(&app.storage);
                            tokio::spawn(async move {
                                let _ = storage.workspaces.save_workspaces(&source, &infos);
                            });
                        }

                        app.set_toast(
                            format!(
                                "Agent dispatched: {} via {}",
                                card_title,
                                provider.label()
                            ),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Dispatch failed: {}", e));
                    }
                }
            }
        }
        Action::SaveAgent {
            source_repo,
            profile,
        } => {
            if let Some(ref storage) = app.storage.agent_profiles {
                if let Err(e) = storage.save_agent(&profile) {
                    app.status_message = Some(format!("Save agent failed: {}", e));
                } else {
                    // Reload agents for this project
                    if let Ok(agents) = storage.load_agents(&source_repo) {
                        app.agent_profiles = agents;
                    }
                    app.set_toast(
                        format!("Agent saved: {}", profile.name),
                        ToastLevel::Success,
                    );
                }
            }
        }
        Action::DeleteAgent(id) => {
            let repo = app.current_workspace().map(|ws| ws.source_repo.clone());
            if let Some(ref storage) = app.storage.agent_profiles {
                if let Err(e) = storage.delete_agent(id) {
                    app.status_message = Some(format!("Delete agent failed: {}", e));
                } else {
                    if let Some(ref repo) = repo
                        && let Ok(agents) = storage.load_agents(repo)
                    {
                        app.agent_profiles = agents;
                    }
                    app.set_toast("Agent deleted".to_string(), ToastLevel::Success);
                }
            }
        }
        Action::SyncAgentToRepo(id) => {
            let ws_info = app.current_workspace().map(|ws| {
                (ws.path.clone(), ws.source_repo.clone())
            });
            let agent_data = app
                .agent_profiles
                .iter()
                .find(|a| a.id == Some(id))
                .map(|a| (a.name.clone(), a.provider.clone(), a.role.clone()));

            if let Some((ws_path, repo)) = ws_info
                && let Some((name, provider_str, role)) = agent_data
            {
                let provider = AIProvider::from_label(&provider_str);
                match materialize_agent_config(&ws_path, &name, &provider, &role, Some(&app.provider_manager)) {
                    Ok(()) => {
                        if let Some(ref storage) = app.storage.agent_profiles {
                            let _ = storage.mark_synced(id);
                            if let Ok(agents) = storage.load_agents(&repo) {
                                app.agent_profiles = agents;
                            }
                        }
                        app.set_toast(
                            format!("Agent synced: {}", name),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Sync failed: {}", e));
                    }
                }
            }
        }
        Action::ScanRepoAgents => {
            if let Some(ws) = app.current_workspace() {
                let source_repo = ws.source_repo.clone();

                // Scan provider agent directories for .md files — all come from ProviderManager.
                let provider_dirs: Vec<(String, String)> = app
                    .provider_manager
                    .all()
                    .iter()
                    .filter_map(|config| {
                        config
                            .agent_dir
                            .as_ref()
                            .map(|d| (d.clone(), config.name.clone()))
                    })
                    .collect();

                let mut discovered: Vec<(String, String, String, bool)> = Vec::new();

                for (dir, provider_label) in &provider_dirs {
                    let agent_dir = source_repo.join(dir);
                    if let Ok(entries) = std::fs::read_dir(&agent_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().is_some_and(|e| e == "md")
                                && let Some(stem) = path.file_stem()
                            {
                                let name = stem.to_string_lossy().to_string();
                                let role =
                                    std::fs::read_to_string(&path).unwrap_or_default();
                                let exists = app.agent_profiles.iter().any(|a| {
                                    a.name == name && a.provider == *provider_label
                                });
                                discovered.push((
                                    name,
                                    provider_label.clone(),
                                    role,
                                    exists,
                                ));
                            }
                        }
                    }
                }

                if discovered.is_empty() {
                    app.set_toast(
                        "No agent files found in repo".to_string(),
                        ToastLevel::Info,
                    );
                } else {
                    // Pre-select only new agents (not already in DB)
                    let selected: Vec<bool> =
                        discovered.iter().map(|(_, _, _, exists)| !exists).collect();
                    app.active_dialog = Some(DialogState::ImportAgents {
                        discovered,
                        selected,
                        cursor: 0,
                    });
                    app.mode = AppMode::ImportAgents;
                }
            }
        }
        Action::ImportAgents(agents_to_import) => {
            if let Some(ws) = app.current_workspace() {
                let source_repo = ws.source_repo.clone();
                if let Some(ref storage) = app.storage.agent_profiles {
                    let mut imported = 0;
                    for (name, provider_label, role) in &agents_to_import {
                        let profile = piki_core::storage::AgentProfile {
                            id: None,
                            source_repo: source_repo.to_string_lossy().to_string(),
                            name: name.clone(),
                            provider: provider_label.clone(),
                            role: role.clone(),
                            version: 0,
                            last_synced_at: None,
                        };
                        if storage.save_agent(&profile).is_ok() {
                            imported += 1;
                            // Mark as synced — the file already exists in repo
                            if let Ok(agents) = storage.load_agents(&source_repo)
                                && let Some(saved) = agents
                                    .iter()
                                    .find(|a| a.name == *name && a.provider == *provider_label)
                                && let Some(id) = saved.id
                            {
                                let _ = storage.mark_synced(id);
                            }
                        }
                    }
                    // Reload agents
                    if let Ok(agents) = storage.load_agents(&source_repo) {
                        app.agent_profiles = agents;
                    }
                    app.set_toast(
                        format!("Imported {} agent(s)", imported),
                        ToastLevel::Success,
                    );
                }
            }
            // Return to manage agents dialog
            app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
            app.mode = AppMode::ManageAgents;
        }

        Action::ChatSendMessage => {
            let input = std::mem::take(&mut app.chat_panel.input);
            let input = input.trim().to_string();
            if input.is_empty() || app.chat_panel.streaming || app.chat_panel.config.model.is_empty()
            {
                if app.chat_panel.config.model.is_empty() {
                    app.set_toast("No model selected. Press Tab to pick one.", ToastLevel::Error);
                }
                return Ok(());
            }

            // Append user message
            app.chat_panel.messages.push(piki_core::chat::ChatMessage {
                role: piki_core::chat::ChatRole::User,
                content: input,
                tool_calls: None,
                tool_call_id: None,
            });
            app.chat_panel.input_cursor = 0;
            app.chat_panel.streaming = true;
            app.chat_panel.current_response.clear();

            let model = app.chat_panel.config.model.clone();
            let base_url = app.chat_panel.config.base_url.clone();
            let server_type = app.chat_panel.config.server_type;

            if app.chat_panel.agent_mode {
                // ── Agent mode: use AgentLoop with tools ──
                let messages = app.chat_panel.messages.clone();
                let system_prompt = app.chat_panel.config.system_prompt.clone();
                let event_tx = app.agent_event_tx.clone();

                // Get workspace path for tool context
                let ws_path = if !app.workspaces.is_empty() {
                    app.workspaces[app.active_workspace].info.path.clone()
                } else {
                    std::env::current_dir().unwrap_or_default()
                };
                let source_repo = ws_path.clone();

                tracing::info!(
                    model = %model,
                    base_url = %base_url,
                    server = %server_type.label(),
                    agent = true,
                    "TUI: sending agent message"
                );

                let client: Box<dyn piki_api_client::ChatClient> = match server_type {
                    piki_core::chat::ChatServerType::Ollama => {
                        Box::new(piki_api_client::OllamaClient::new(&base_url))
                    }
                    piki_core::chat::ChatServerType::LlamaCpp => {
                        Box::new(piki_api_client::LlamaCppClient::new(&base_url))
                    }
                };

                let registry = piki_agent::ToolRegistry::default_all();
                let context = piki_agent::ToolContext {
                    workspace_path: ws_path,
                    source_repo,
                };

                tokio::spawn(async move {
                    let mut agent = piki_agent::AgentLoop::new(
                        client, model, registry, context,
                    );
                    if let Err(e) = agent.run(messages, system_prompt, event_tx.clone()).await {
                        tracing::error!(error = %e, "Agent loop error");
                        let _ = event_tx.send(piki_agent::AgentEvent::Error(e.to_string()));
                    }
                });
            } else {
                // ── Plain chat mode (existing behavior) ──
                let tx = app.chat_token_tx.clone();

                let mut role_contents: Vec<(&str, String)> = Vec::new();
                if let Some(ref sys) = app.chat_panel.config.system_prompt
                    && !sys.is_empty()
                {
                    role_contents.push(("system", sys.clone()));
                }
                for msg in &app.chat_panel.messages {
                    let role = match msg.role {
                        piki_core::chat::ChatRole::System => "system",
                        piki_core::chat::ChatRole::User => "user",
                        piki_core::chat::ChatRole::Assistant => "assistant",
                        piki_core::chat::ChatRole::Tool => "tool",
                    };
                    role_contents.push((role, msg.content.clone()));
                }

                tracing::info!(
                    model = %model,
                    base_url = %base_url,
                    server = %server_type.label(),
                    msg_count = role_contents.len(),
                    "TUI: sending chat message"
                );

                match server_type {
                    piki_core::chat::ChatServerType::Ollama => {
                        let msgs: Vec<piki_api_client::OllamaMessage> = role_contents
                            .into_iter()
                            .map(|(r, c)| piki_api_client::OllamaMessage {
                                role: r.to_string(),
                                content: c,
                                tool_calls: None,
                            })
                            .collect();
                        let client = piki_api_client::OllamaClient::new(&base_url);
                        tokio::spawn(async move {
                            if let Err(e) = client.chat_stream(&model, &msgs, tx).await {
                                tracing::error!(error = %e, "Ollama chat_stream error");
                            }
                        });
                    }
                    piki_core::chat::ChatServerType::LlamaCpp => {
                        let msgs: Vec<piki_api_client::LlamaCppMessage> = role_contents
                            .into_iter()
                            .map(|(r, c)| piki_api_client::LlamaCppMessage {
                                role: r.to_string(),
                                content: c,
                                tool_calls: None,
                                tool_call_id: None,
                            })
                            .collect();
                        let client = piki_api_client::LlamaCppClient::new(&base_url);
                        tokio::spawn(async move {
                            if let Err(e) = client.chat_stream(&model, &msgs, tx).await {
                                tracing::error!(error = %e, "llama.cpp chat_stream error");
                            }
                        });
                    }
                }
            }
        }

        Action::ChatLoadModels => {
            let base_url = app.chat_panel.config.base_url.clone();
            let server_type = app.chat_panel.config.server_type;
            let status_tx = app.status_tx.clone();
            let chat_tx = app.chat_token_tx.clone();
            tracing::debug!(base_url = %base_url, server = %server_type.label(), "TUI: loading chat models");

            match server_type {
                piki_core::chat::ChatServerType::Ollama => {
                    tokio::spawn(async move {
                        let client = piki_api_client::OllamaClient::new(&base_url);
                        match client.list_models().await {
                            Ok(models) => {
                                let names: Vec<String> =
                                    models.into_iter().map(|m| m.name).collect();
                                let payload = format!("__MODELS__{}", names.join("\n"));
                                let _ = chat_tx
                                    .send(piki_api_client::ChatStreamEvent::Done(payload));
                            }
                            Err(e) => {
                                let msg = format!("{e}. Is Ollama running? (ollama serve)");
                                let _ = status_tx.send(msg);
                            }
                        }
                    });
                }
                piki_core::chat::ChatServerType::LlamaCpp => {
                    tokio::spawn(async move {
                        let client = piki_api_client::LlamaCppClient::new(&base_url);
                        match client.list_models().await {
                            Ok(models) => {
                                let names: Vec<String> =
                                    models.into_iter().map(|m| m.id).collect();
                                let payload = format!("__MODELS__{}", names.join("\n"));
                                let _ = chat_tx
                                    .send(piki_api_client::ChatStreamEvent::Done(payload));
                            }
                            Err(e) => {
                                let msg = format!(
                                    "{e}. Is llama-server running? (llama-server -m model.gguf)"
                                );
                                let _ = status_tx.send(msg);
                            }
                        }
                    });
                }
            }
        }
    }
    Ok(())
}

/// Write agent role/instructions to the provider's standard subagent config path in the worktree.
fn materialize_agent_config(
    worktree_path: &std::path::Path,
    agent_name: &str,
    provider: &AIProvider,
    role: &str,
    provider_manager: Option<&piki_core::providers::ProviderManager>,
) -> anyhow::Result<()> {
    let filename = format!("{}.md", agent_name);
    let dir = if let AIProvider::Custom(name) = provider
        && let Some(mgr) = provider_manager
        && let Some(config) = mgr.get(name)
        && let Some(agent_dir) = &config.agent_dir
    {
        agent_dir.clone()
    } else {
        return Ok(());
    };
    let agent_dir = worktree_path.join(dir);
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join(filename), role)?;
    Ok(())
}

