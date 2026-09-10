//! Project actions backing the Projects overlay (`prefix ctrl-p`): save or
//! delete a project, and adopt a plain-directory member as a workspace.
//! Storage calls are plain synchronous SQLite (fast, no spawn_blocking
//! needed); after every mutation the open overlay's list is reloaded from
//! storage so it reflects reality.

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, ToastLevel};
use crate::dialog_state::DialogState;
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::SaveProject(mut project) => {
            let Some(store) = app.storage.projects.as_ref() else {
                app.set_toast("project storage unavailable", ToastLevel::Error);
                return Ok(());
            };
            // New projects go to the end of the list: the storage upserts
            // whatever order it is given, so assign max+1 here.
            if project.id.is_none() {
                project.order = store
                    .list_projects()
                    .iter()
                    .map(|p| p.order)
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(0);
            }
            if let Err(e) = store.save_project(&project) {
                app.set_toast(format!("project save failed: {e}"), ToastLevel::Error);
            }
            reload_projects(app);
        }
        Action::DeleteProject(id) => {
            let Some(store) = app.storage.projects.as_ref() else {
                app.set_toast("project storage unavailable", ToastLevel::Error);
                return Ok(());
            };
            if let Err(e) = store.delete_project(id) {
                app.set_toast(format!("project delete failed: {e}"), ToastLevel::Error);
            }
            reload_projects(app);
        }
        Action::ProjectAdoptDirectory { path } => {
            // A plain-directory member becomes a Simple workspace named after
            // its basename. Membership stores only the path, so the project
            // row upgrades to a workspace row with no migration.
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            match manager.create_simple(&name, "", "", None, &path).await {
                Ok(info) => super::workspace::finish_workspace_creation(app, info),
                Err(e) => app.set_toast(format!("Error: {e}"), ToastLevel::Error),
            }
        }
        other => unreachable!("non-project action routed to action::projects: {other:?}"),
    }
    Ok(())
}

/// Refresh the open Projects overlay from storage after a save/delete, so the
/// list shows what was actually persisted. Drops expansion state of projects
/// that no longer exist and clamps the selection.
fn reload_projects(app: &mut App) {
    let fresh = app
        .storage
        .projects
        .as_ref()
        .map(|s| s.list_projects())
        .unwrap_or_default();
    if let Some(DialogState::Projects {
        projects,
        selected,
        expanded,
        ..
    }) = &mut app.active_dialog
    {
        expanded.retain(|id| fresh.iter().any(|p| p.id == Some(*id)));
        *projects = fresh;
        let rows = crate::dialog_state::project_rows(projects, expanded);
        *selected = (*selected).min(rows.len().saturating_sub(1));
    }
}
