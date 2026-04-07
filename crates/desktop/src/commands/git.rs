use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;
use piki_core::ChangedFile;
use piki_core::workspace::manager::WorkspaceManager;

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct MergeResult {
    pub success: bool,
    pub message: String,
    pub conflicts: Vec<String>,
}

#[tauri::command]
pub async fn get_changed_files(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<ChangedFile>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    piki_core::git::get_changed_files(&ws_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn git_stage(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["add", "--", &file_path])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git add: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn git_unstage(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["reset", "HEAD", "--", &file_path])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git reset: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git reset failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn git_commit(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    message: String,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["commit", "-m", &message])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git commit: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn git_push(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["push"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git push: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git push failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn git_merge(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    strategy: String,
) -> Result<MergeResult, String> {
    let (ws_path, source_repo, branch) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        let ws = &app.workspaces[workspace_idx];
        (
            ws.info.path.clone(),
            ws.info.source_repo.clone(),
            ws.info.branch.clone(),
        )
    };

    // Check for uncommitted changes
    let status = piki_core::shell_env::command("git")
        .args(["status", "--porcelain"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let status_str = String::from_utf8_lossy(&status.stdout);
    if !status_str.trim().is_empty() {
        return Ok(MergeResult {
            success: false,
            message: "Workspace has uncommitted changes. Commit or stash first.".into(),
            conflicts: Vec::new(),
        });
    }

    let main_branch = WorkspaceManager::detect_main_branch(&source_repo).await;

    if strategy == "rebase" {
        // Rebase workspace branch onto main
        let rebase = piki_core::shell_env::command("git")
            .args(["rebase", &main_branch])
            .current_dir(&ws_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if rebase.status.success() {
            return Ok(MergeResult {
                success: true,
                message: format!("Rebased '{}' onto {}", branch, main_branch),
                conflicts: Vec::new(),
            });
        }

        let conflicts = detect_conflict_files(&ws_path).await;
        if !conflicts.is_empty() {
            return Ok(MergeResult {
                success: false,
                message: "Rebase conflicts detected".into(),
                conflicts,
            });
        }

        // Other rebase error — abort
        let _ = piki_core::shell_env::command("git")
            .args(["rebase", "--abort"])
            .current_dir(&ws_path)
            .output()
            .await;
        let stderr = String::from_utf8_lossy(&rebase.stderr);
        return Ok(MergeResult {
            success: false,
            message: format!("Rebase failed: {}", stderr.trim()),
            conflicts: Vec::new(),
        });
    }

    // Merge strategy: checkout main in source repo, merge branch
    let src_status = piki_core::shell_env::command("git")
        .args(["status", "--porcelain"])
        .current_dir(&source_repo)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let src_dirty = !String::from_utf8_lossy(&src_status.stdout)
        .trim()
        .is_empty();

    if src_dirty {
        let _ = piki_core::shell_env::command("git")
            .args(["stash", "push", "-m", "piki-desktop-merge-temp"])
            .current_dir(&source_repo)
            .output()
            .await;
    }

    // Save current branch
    let prev_output = piki_core::shell_env::command("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&source_repo)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let prev_branch = String::from_utf8_lossy(&prev_output.stdout)
        .trim()
        .to_string();

    // Checkout main
    let checkout = piki_core::shell_env::command("git")
        .args(["checkout", &main_branch])
        .current_dir(&source_repo)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !checkout.status.success() {
        if src_dirty {
            let _ = piki_core::shell_env::command("git")
                .args(["stash", "pop"])
                .current_dir(&source_repo)
                .output()
                .await;
        }
        let stderr = String::from_utf8_lossy(&checkout.stderr);
        return Ok(MergeResult {
            success: false,
            message: format!("Checkout {} failed: {}", main_branch, stderr.trim()),
            conflicts: Vec::new(),
        });
    }

    // Merge
    let merge = piki_core::shell_env::command("git")
        .args(["merge", &branch])
        .current_dir(&source_repo)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if merge.status.success() {
        // Restore previous state
        if prev_branch != main_branch {
            let _ = piki_core::shell_env::command("git")
                .args(["checkout", &prev_branch])
                .current_dir(&source_repo)
                .output()
                .await;
        }
        if src_dirty {
            let _ = piki_core::shell_env::command("git")
                .args(["stash", "pop"])
                .current_dir(&source_repo)
                .output()
                .await;
        }
        let stdout = String::from_utf8_lossy(&merge.stdout);
        let first = stdout.lines().next().unwrap_or("Merged");
        return Ok(MergeResult {
            success: true,
            message: format!("Merged '{}' into {}: {}", branch, main_branch, first),
            conflicts: Vec::new(),
        });
    }

    // Check for conflicts
    let conflicts = detect_conflict_files(&source_repo).await;
    if !conflicts.is_empty() {
        // Stay on main branch so user can resolve conflicts in source repo
        return Ok(MergeResult {
            success: false,
            message: "Merge conflicts detected — resolve them to continue".into(),
            conflicts,
        });
    }

    // Other error — abort and restore
    let _ = piki_core::shell_env::command("git")
        .args(["merge", "--abort"])
        .current_dir(&source_repo)
        .output()
        .await;
    if prev_branch != main_branch {
        let _ = piki_core::shell_env::command("git")
            .args(["checkout", &prev_branch])
            .current_dir(&source_repo)
            .output()
            .await;
    }
    if src_dirty {
        let _ = piki_core::shell_env::command("git")
            .args(["stash", "pop"])
            .current_dir(&source_repo)
            .output()
            .await;
    }
    let stderr = String::from_utf8_lossy(&merge.stderr);
    Ok(MergeResult {
        success: false,
        message: format!("Merge failed: {}", stderr.trim()),
        conflicts: Vec::new(),
    })
}

#[tauri::command]
pub async fn git_abort_merge(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<(), String> {
    let (ws_path, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        let ws = &app.workspaces[workspace_idx];
        (ws.info.path.clone(), ws.info.source_repo.clone())
    };

    // Try both merge and rebase abort
    let _ = piki_core::shell_env::command("git")
        .args(["merge", "--abort"])
        .current_dir(&source_repo)
        .output()
        .await;
    let _ = piki_core::shell_env::command("git")
        .args(["rebase", "--abort"])
        .current_dir(&ws_path)
        .output()
        .await;

    Ok(())
}

#[tauri::command]
pub async fn git_resolve_conflict(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
    resolution: String,
) -> Result<(), String> {
    let (ws_path, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        let ws = &app.workspaces[workspace_idx];
        (ws.info.path.clone(), ws.info.source_repo.clone())
    };

    // Determine which directory has the conflict (source_repo for merge, ws_path for rebase)
    let dir = if source_repo.join(&file_path).exists() {
        &source_repo
    } else {
        &ws_path
    };

    match resolution.as_str() {
        "ours" => {
            let _ = piki_core::shell_env::command("git")
                .args(["checkout", "--ours", "--", &file_path])
                .current_dir(dir)
                .output()
                .await
                .map_err(|e| e.to_string())?;
        }
        "theirs" => {
            let _ = piki_core::shell_env::command("git")
                .args(["checkout", "--theirs", "--", &file_path])
                .current_dir(dir)
                .output()
                .await
                .map_err(|e| e.to_string())?;
        }
        _ => {} // "staged" = file was manually edited and staged
    }

    // Stage the resolved file
    let output = piki_core::shell_env::command("git")
        .args(["add", "--", &file_path])
        .current_dir(dir)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn git_continue_merge(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<String, String> {
    let (ws_path, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        let ws = &app.workspaces[workspace_idx];
        (ws.info.path.clone(), ws.info.source_repo.clone())
    };

    // Try rebase --continue first (in workspace path)
    let rebase = piki_core::shell_env::command("git")
        .args(["rebase", "--continue"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if rebase.status.success() {
        return Ok("Rebase completed".into());
    }

    // Try merge --continue (commit in source repo)
    let merge = piki_core::shell_env::command("git")
        .args(["commit", "--no-edit"])
        .current_dir(&source_repo)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if merge.status.success() {
        return Ok("Merge completed".into());
    }

    Err(format!(
        "Continue failed: {}",
        String::from_utf8_lossy(&merge.stderr).trim()
    ))
}

async fn detect_conflict_files(dir: &std::path::Path) -> Vec<String> {
    let output = piki_core::shell_env::command("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(dir)
        .output()
        .await;
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            piki_core::git::parse_porcelain_status(&stdout)
                .into_iter()
                .filter(|f| matches!(f.status, piki_core::FileStatus::Conflicted))
                .map(|f| f.path)
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
pub async fn git_stage_all(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["add", "-A"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git add -A: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git add -A failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn git_unstage_all(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["reset", "HEAD"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git reset HEAD: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git reset HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

fn get_ws_path(
    state: &State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<std::path::PathBuf, String> {
    let app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    Ok(app.workspaces[workspace_idx].info.path.clone())
}
