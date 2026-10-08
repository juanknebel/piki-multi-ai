use parking_lot::Mutex;
use tauri::{AppHandle, State};

use piki_core::workspace::watcher::FileWatcher;
use piki_core::{WorkspaceInfo, WorkspaceStatus};

use crate::state::{DesktopApp, DesktopWorkspace, WorkspaceDetail};

/// Register a freshly-created/imported workspace: assign the next display
/// order, push it (with its file watcher) onto `app.workspaces`, and persist
/// the whole list keyed by its `source_repo`. Shared tail of
/// `create_workspace`, `create_github_workspace` and `import_existing_worktree`.
fn register_new_workspace(app: &mut DesktopApp, mut info: WorkspaceInfo) -> WorkspaceInfo {
    let watcher = FileWatcher::new(info.path.clone(), info.name.clone()).ok();
    let order = app
        .workspaces
        .iter()
        .map(|ws| ws.info.order)
        .max()
        .unwrap_or(0)
        + 1;
    info.order = order;

    app.workspaces.push(DesktopWorkspace {
        info: info.clone(),
        status: WorkspaceStatus::Idle,
        changed_files: Vec::new(),
        ahead_behind: None,
        branch: None,
        tabs: Vec::new(),
        active_tab: 0,
        watcher,
        file_index: None,
    });

    let all_infos: Vec<WorkspaceInfo> = app.workspaces.iter().map(|ws| ws.info.clone()).collect();
    let _ = app
        .storage
        .workspaces
        .save_workspaces(&info.source_repo, &all_infos);

    info
}

#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Vec<WorkspaceInfo>, String> {
    let app = state.lock();
    Ok(app.workspaces.iter().map(|ws| ws.info.clone()).collect())
}

/// Returns the suggested default destination folder for a GitHub clone:
/// `<data_dir>/repos`. The dialog pre-fills its "Clone into" input with
/// this so the user has a sensible starting point, but can override.
#[tauri::command]
pub async fn default_clone_destination(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<String, String> {
    let app = state.lock();
    Ok(app.paths.repos_dir().to_string_lossy().to_string())
}

#[tauri::command]
pub async fn switch_workspace(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
) -> Result<WorkspaceDetail, String> {
    // Scope the lock: set active workspace and extract path
    let ws_path = {
        let mut app = state.lock();
        if index >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.active_workspace = index;
        // Re-walk on the next Ctrl+F: cheap, and it covers anything the
        // watcher could not see while the workspace was in the background.
        app.workspaces[index].file_index = None;
        let path = app.workspaces[index].info.path.clone();
        // Persist the active workspace so the next startup focuses it.
        if let Some(prefs) = app.storage.ui_prefs.as_ref() {
            let path_str = path.to_string_lossy().to_string();
            let _ = prefs.set_preference("last_focused_workspace", &path_str);
        }
        path
    };

    // Async git operations (no lock held)
    let files = piki_core::git::get_changed_files(&ws_path)
        .await
        .unwrap_or_default();
    let ahead_behind = piki_core::git::get_ahead_behind(&ws_path).await;

    // Re-lock to update state and return detail
    let mut app = state.lock();
    if index < app.workspaces.len() {
        app.workspaces[index].changed_files = files;
        app.workspaces[index].ahead_behind = ahead_behind;
        // The switch makes this workspace's active tab visible — acknowledge
        // its agent's "unseen news" marker, mirroring the TUI's event loop,
        // and tell the frontend once the lock is gone.
        let ws = &app.workspaces[index];
        let acked = ws
            .tabs
            .get(ws.active_tab)
            .filter(|tab| crate::events::acknowledge_agent_attention(tab))
            .map(|tab| tab.id.clone());
        let detail = ws.to_detail();
        drop(app);
        if let Some(tab_id) = acked {
            crate::events::emit_agent_ack(&app_handle, &tab_id);
        }
        Ok(detail)
    } else {
        Err("Workspace removed during switch".to_string())
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    name: String,
    description: String,
    prompt: String,
    dir: String,
    ws_type: String,
    kanban_path: Option<String>,
) -> Result<WorkspaceInfo, String> {
    // Extract manager info with scoped lock
    let manager = {
        let app = state.lock();
        piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone())
    };

    let source_repo = std::path::PathBuf::from(&dir);

    // Async workspace creation (no lock held)
    let info = match ws_type.as_str() {
        "Simple" => manager
            .create_simple(
                &name,
                &description,
                &prompt,
                kanban_path.clone(),
                &source_repo,
            )
            .await
            .map_err(|e| e.to_string())?,
        "Project" => manager
            .create_project(
                &name,
                &description,
                &prompt,
                kanban_path.clone(),
                &source_repo,
            )
            .await
            .map_err(|e| e.to_string())?,
        _ => manager
            .create(&name, &description, &prompt, kanban_path, &source_repo)
            .await
            .map_err(|e| e.to_string())?,
    };

    let mut app = state.lock();
    Ok(register_new_workspace(&mut app, info))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_github_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    name: String,
    description: String,
    prompt: String,
    github_url: String,
    destination_dir: String,
    kanban_path: Option<String>,
) -> Result<WorkspaceInfo, String> {
    let manager = {
        let app = state.lock();
        piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone())
    };

    let destination_path = std::path::PathBuf::from(destination_dir);
    let info = manager
        .create_from_github(
            &name,
            &description,
            &prompt,
            kanban_path,
            &github_url,
            &destination_path,
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut app = state.lock();
    Ok(register_new_workspace(&mut app, info))
}

/// One entry from `git worktree list`, for the "Load Existing Worktree"
/// picker in the Create Worktree dialog.
#[derive(serde::Serialize)]
pub struct ExistingWorktreeInfo {
    pub path: String,
    pub branch: String,
}

/// List git worktrees for the repo backing `source_repo` (a GitHub-origin
/// workspace's `source_repo`), excluding ones already registered as
/// workspaces. Mirrors the TUI's `Action::ListWorktrees`.
#[tauri::command]
pub async fn list_worktrees(
    state: State<'_, Mutex<DesktopApp>>,
    source_repo: String,
) -> Result<Vec<ExistingWorktreeInfo>, String> {
    let (manager, registered) = {
        let app = state.lock();
        (
            piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone()),
            app.workspaces
                .iter()
                .map(|ws| ws.info.path.clone())
                .collect::<Vec<_>>(),
        )
    };
    let found = manager
        .list_worktrees(&std::path::PathBuf::from(&source_repo))
        .await
        .map_err(|e| e.to_string())?;
    Ok(found
        .into_iter()
        .filter(|w| !registered.contains(&w.path))
        .map(|w| ExistingWorktreeInfo {
            path: w.path.to_string_lossy().to_string(),
            branch: w.branch,
        })
        .collect())
}

/// Register an already-existing worktree directory (from `list_worktrees`)
/// as a new workspace, without shelling out to `git worktree add`. Mirrors
/// the TUI's `Action::ImportExistingWorktree`.
#[tauri::command]
pub async fn import_existing_worktree(
    state: State<'_, Mutex<DesktopApp>>,
    source_repo: String,
    path: String,
    branch: String,
) -> Result<WorkspaceInfo, String> {
    let manager = {
        let app = state.lock();
        piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone())
    };
    let name = branch.rsplit('/').next().unwrap_or(&branch).to_string();
    let info = manager
        .import_existing_worktree(
            &name,
            std::path::PathBuf::from(&path),
            std::path::PathBuf::from(&source_repo),
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut app = state.lock();
    Ok(register_new_workspace(&mut app, info))
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
) -> Result<(), String> {
    // Scope the lock: remove workspace and extract info for cleanup
    let (ws_name, ws_source_repo, manager) = {
        let mut app = state.lock();

        if index >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }

        let mut ws = app.workspaces.remove(index);
        if app.active_workspace >= app.workspaces.len() && !app.workspaces.is_empty() {
            app.active_workspace = app.workspaces.len() - 1;
        }

        // End the workspace's processes explicitly (the confirm dialog says
        // they will be terminated): its worktree is about to be removed, and
        // a plain drop would only *detach* daemon sessions, leaving orphans
        // whose cwd no longer exists.
        let daemon = app.session_daemon.clone();
        for tab in ws.tabs.iter_mut() {
            let was_remote = tab.pty.as_ref().is_some_and(|p| p.is_remote());
            if let Some(pty) = tab.pty.as_mut() {
                let _ = pty.kill();
            }
            if was_remote && let Some(d) = daemon.as_ref() {
                crate::session::remove_session(d, &tab.id);
            }
        }

        // Save to storage — use the removed workspace's source_repo as the key
        let all_infos: Vec<WorkspaceInfo> = app.workspaces.iter().map(|w| w.info.clone()).collect();
        let _ = app
            .storage
            .workspaces
            .save_workspaces(&ws.info.source_repo, &all_infos);

        let manager =
            piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone());
        (ws.info.name, ws.info.source_repo, manager)
    };

    // Async worktree removal (no lock held)
    let _ = manager.remove(&ws_name, &ws_source_repo).await;

    Ok(())
}

#[tauri::command]
pub async fn list_project_subdirs(
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
) -> Result<Vec<String>, String> {
    let ws_path = {
        let app = state.lock();
        if index >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[index].info.path.clone()
    };

    Ok(piki_core::workspace::manager::list_subdirs(&ws_path).await)
}

#[tauri::command]
pub async fn update_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
    prompt: Option<String>,
    description: Option<String>,
    kanban_path: Option<String>,
) -> Result<(), String> {
    let mut app = state.lock();
    if index >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }

    let ws = &mut app.workspaces[index];
    if let Some(p) = prompt {
        ws.info.prompt = p;
    }
    if let Some(d) = description {
        ws.info.description = d;
    }
    if let Some(k) = kanban_path {
        ws.info.kanban_path = if k.is_empty() { None } else { Some(k) };
    }

    // Persist — use the updated workspace's source_repo as the key
    let source_repo = app.workspaces[index].info.source_repo.clone();
    let all_infos: Vec<WorkspaceInfo> = app.workspaces.iter().map(|w| w.info.clone()).collect();
    let _ = app
        .storage
        .workspaces
        .save_workspaces(&source_repo, &all_infos);

    Ok(())
}

/// Family collapse state, keyed by worktree-family identifier (a workspace's
/// `source_repo` path as a string) — mirrors the TUI's `collapsed_groups`.
#[tauri::command]
pub async fn get_collapsed_groups(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Vec<String>, String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_ref()
        .ok_or("Storage not available")?;
    let groups = prefs
        .get_collapsed_groups()
        .map_err(|e| format!("Failed to load collapsed groups: {e}"))?;
    Ok(groups.into_iter().collect())
}

#[tauri::command]
pub async fn set_collapsed_groups(
    state: State<'_, Mutex<DesktopApp>>,
    groups: Vec<String>,
) -> Result<(), String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_ref()
        .ok_or("Storage not available")?;
    prefs
        .set_collapsed_groups(&groups.into_iter().collect())
        .map_err(|e| format!("Failed to save collapsed groups: {e}"))
}

/// Serializable view of one `piki_core::projects::tree::ProjectTreeRow`.
///
/// Flattened on purpose: the frontend renders rows, it does not re-derive the
/// tree. Grouping (which repo a checkout belongs to, what is collapsed, where
/// the synthetic buckets go) is decided once, in core, so this app and the TUI
/// cannot drift apart — the way they did when the family rule lived in
/// TypeScript too and the PR-review group was never implemented here.
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProjectTreeRowDto {
    /// A top-level header: a real project, or one of the synthetic buckets.
    Project {
        /// Collapse key.
        key: String,
        collapsed: bool,
        /// Workspaces underneath it, across all its repos.
        checkouts: usize,
        /// `Some(id)` for a real project (`None` for an unsaved one);
        /// `None` for a bucket.
        project_id: Option<i64>,
        /// Project name, or "" for a bucket (the frontend names those).
        name: String,
        /// Palette index of a real project; `None` for a bucket.
        color: Option<u8>,
        /// "project" | "prReview" | "unassigned"
        bucket: &'static str,
    },
    /// A repository group. Synthetic — derived from its checkouts'
    /// `source_repo`, never stored.
    Repo {
        key: String,
        collapsed: bool,
        checkouts: usize,
        /// Absolute repository root.
        root: String,
        /// Display name for the root.
        display: String,
    },
    /// A loaded workspace: `index` points into `list_workspaces`' order.
    Checkout {
        index: usize,
        /// "primary" (the original clone) | "worktree"
        kind: &'static str,
        /// 2 under a repo group, 1 when the row hangs off its header.
        depth: u8,
    },
    /// A member path that is neither a loaded workspace nor a repo root.
    Dir { path: String },
}

/// The sidebar's rows in render order: the project tree, with collapse state
/// already applied. The ONE sidebar model — there is no flat workspace view.
#[tauri::command]
pub async fn project_tree(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Vec<ProjectTreeRowDto>, String> {
    use piki_core::projects::tree::{Bucket, CheckoutKind, ProjectTreeRow};

    let app = state.lock();
    let collapsed = app
        .storage
        .ui_prefs
        .as_ref()
        .and_then(|p| p.get_collapsed_groups().ok())
        .unwrap_or_default();
    let projects = app
        .storage
        .projects
        .as_ref()
        .map(|s| s.list_projects())
        .unwrap_or_default();
    let infos: Vec<piki_core::WorkspaceInfo> =
        app.workspaces.iter().map(|w| w.info.clone()).collect();

    Ok(
        piki_core::projects::tree::project_tree(&projects, &infos, &collapsed)
            .into_iter()
            .map(|row| match row {
                ProjectTreeRow::Project {
                    bucket,
                    key,
                    collapsed,
                    checkouts,
                } => {
                    let (project_id, name, color, tag) = match bucket {
                        Bucket::Project(i) => {
                            let p = &projects[i];
                            (p.id, p.name.clone(), Some(p.clamped_color()), "project")
                        }
                        Bucket::PrReview => (None, String::new(), None, "prReview"),
                        Bucket::Unassigned => (None, String::new(), None, "unassigned"),
                    };
                    ProjectTreeRowDto::Project {
                        key,
                        collapsed,
                        checkouts,
                        project_id,
                        name,
                        color,
                        bucket: tag,
                    }
                }
                ProjectTreeRow::Repo {
                    root,
                    display,
                    key,
                    collapsed,
                    checkouts,
                    ..
                } => ProjectTreeRowDto::Repo {
                    key,
                    collapsed,
                    checkouts,
                    root: root.to_string_lossy().into_owned(),
                    display,
                },
                ProjectTreeRow::Checkout {
                    workspace_index,
                    kind,
                    depth,
                    ..
                } => ProjectTreeRowDto::Checkout {
                    index: workspace_index,
                    kind: match kind {
                        CheckoutKind::Primary => "primary",
                        CheckoutKind::Worktree => "worktree",
                    },
                    depth,
                },
                ProjectTreeRow::Dir { path, .. } => ProjectTreeRowDto::Dir {
                    path: path.to_string_lossy().into_owned(),
                },
            })
            .collect(),
    )
}
