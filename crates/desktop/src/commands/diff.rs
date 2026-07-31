use crate::state::DesktopApp;
use parking_lot::Mutex;
use tauri::State;

// Types and parsers live in core: they are pure string handling with nothing
// desktop-specific, and the TUI needs them too.
use piki_core::diff::{
    ConflictDiff, LegacyDiffLine, SideBySideDiff, parse_conflict_markers, parse_legacy_diff_lines,
    parse_side_by_side,
};

// ── Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn get_side_by_side_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
    staged: bool,
) -> Result<SideBySideDiff, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };
    let branch = piki_core::git::get_current_branch(&ws_path)
        .await
        .unwrap_or_default();

    let mut args = vec![
        "diff".to_string(),
        "--no-color".to_string(),
        "-U3".to_string(),
    ];
    if staged {
        args.push("--cached".to_string());
    }
    args.push("--".to_string());
    args.push(file_path.clone());

    let output = piki_core::shell_env::command("git")
        .args(&args)
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git diff failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // For untracked/new files, use --no-index
    let diff_text = if stdout.is_empty() {
        let show = piki_core::shell_env::command("git")
            .args([
                "diff",
                "--no-color",
                "-U3",
                "--no-index",
                "/dev/null",
                &file_path,
            ])
            .current_dir(&ws_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        String::from_utf8_lossy(&show.stdout).to_string()
    } else {
        stdout.to_string()
    };

    let left_title = if staged { "INDEX" } else { "HEAD" };
    let right_title = if staged { "STAGED" } else { &branch };

    Ok(parse_side_by_side(
        &diff_text,
        left_title,
        right_title,
        &file_path,
    ))
}

#[tauri::command]
pub async fn get_commit_side_by_side_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    sha: String,
) -> Result<Vec<SideBySideDiff>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let output = piki_core::shell_env::command("git")
        .args(["show", "--no-color", "-U3", "-p", "--format=", &sha])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git show failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let short_sha = &sha[..sha.len().min(8)];

    // Split by "diff --git" to get per-file diffs
    let mut diffs = Vec::new();
    let mut current_file = String::new();
    let mut current_chunk = String::new();

    for line in stdout.lines() {
        if line.starts_with("diff --git") {
            if !current_chunk.is_empty() {
                diffs.push(parse_side_by_side(
                    &current_chunk,
                    &format!("{short_sha}^"),
                    short_sha,
                    &current_file,
                ));
            }
            // Extract file path: "diff --git a/path b/path"
            current_file = line.split(" b/").nth(1).unwrap_or("unknown").to_string();
            current_chunk = format!("{line}\n");
        } else {
            current_chunk.push_str(line);
            current_chunk.push('\n');
        }
    }
    if !current_chunk.is_empty() {
        diffs.push(parse_side_by_side(
            &current_chunk,
            &format!("{short_sha}^"),
            short_sha,
            &current_file,
        ));
    }

    Ok(diffs)
}

#[tauri::command]
pub async fn get_conflict_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
) -> Result<ConflictDiff, String> {
    let (ws_path, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        let ws = &app.workspaces[workspace_idx];
        (ws.info.path.clone(), ws.info.source_repo.clone())
    };

    // Read the file with conflict markers
    let dir = if source_repo.join(&file_path).exists() {
        &source_repo
    } else {
        &ws_path
    };

    let content = tokio::fs::read_to_string(dir.join(&file_path))
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let regions = parse_conflict_markers(&content);

    Ok(ConflictDiff {
        file_path,
        ours_title: "OURS (current)".to_string(),
        theirs_title: "THEIRS (incoming)".to_string(),
        regions,
    })
}

// Keep legacy commands for backward compatibility
#[tauri::command]
pub async fn get_file_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
    staged: bool,
) -> Result<Vec<LegacyDiffLine>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let mut args = vec!["diff", "--no-color"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(&file_path);

    let output = piki_core::shell_env::command("git")
        .args(&args)
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git diff failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        let show = piki_core::shell_env::command("git")
            .args(["diff", "--no-color", "--no-index", "/dev/null", &file_path])
            .current_dir(&ws_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let s = String::from_utf8_lossy(&show.stdout);
        return Ok(parse_legacy_diff_lines(&s));
    }
    Ok(parse_legacy_diff_lines(&stdout))
}

#[tauri::command]
pub async fn get_commit_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    sha: String,
) -> Result<Vec<LegacyDiffLine>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let output = piki_core::shell_env::command("git")
        .args(["show", "--no-color", "--stat", "-p", &sha])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git show failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_legacy_diff_lines(&stdout))
}
