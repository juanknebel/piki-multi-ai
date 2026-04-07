use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;
use crate::state::DesktopApp;

// ── Types ──────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct SideBySideDiff {
    pub left_title: String,
    pub right_title: String,
    pub file_path: String,
    pub hunks: Vec<DiffHunk>,
    pub stats: DiffStats,
}

#[derive(Serialize, Clone)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Serialize, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub pairs: Vec<DiffPair>,
}

#[derive(Serialize, Clone)]
pub struct DiffPair {
    pub left: Option<DiffSide>,
    pub right: Option<DiffSide>,
    pub pair_type: String, // "context", "modified", "added", "deleted"
}

#[derive(Serialize, Clone)]
pub struct DiffSide {
    pub line_num: u32,
    pub content: String,
}

#[derive(Serialize, Clone)]
pub struct ConflictDiff {
    pub file_path: String,
    pub ours_title: String,
    pub theirs_title: String,
    pub regions: Vec<ConflictRegion>,
}

#[derive(Serialize, Clone)]
pub struct ConflictRegion {
    pub region_type: String, // "common", "conflict"
    pub ours_lines: Vec<String>,
    pub theirs_lines: Vec<String>,
    pub base_lines: Vec<String>,
}

// ── Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn get_side_by_side_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
    staged: bool,
) -> Result<SideBySideDiff, String> {
    let (ws_path, branch) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        let ws = &app.workspaces[workspace_idx];
        (ws.info.path.clone(), ws.info.branch.clone())
    };

    let mut args = vec!["diff".to_string(), "--no-color".to_string(), "-U3".to_string()];
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
            .args(["diff", "--no-color", "-U3", "--no-index", "/dev/null", &file_path])
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
            current_file = line
                .split(" b/")
                .nth(1)
                .unwrap_or("unknown")
                .to_string();
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

// ── Parsers ────────────────────────────────────────────

fn parse_side_by_side(
    raw: &str,
    left_title: &str,
    right_title: &str,
    file_path: &str,
) -> SideBySideDiff {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_num: u32 = 0;
    let mut new_num: u32 = 0;
    let mut pending_dels: Vec<(u32, String)> = Vec::new();
    let mut pending_adds: Vec<(u32, String)> = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;

    let flush_pending =
        |dels: &mut Vec<(u32, String)>, adds: &mut Vec<(u32, String)>, hunk: &mut DiffHunk| {
            let max_len = dels.len().max(adds.len());
            for i in 0..max_len {
                let left = dels.get(i).map(|(n, c)| DiffSide {
                    line_num: *n,
                    content: c.clone(),
                });
                let right = adds.get(i).map(|(n, c)| DiffSide {
                    line_num: *n,
                    content: c.clone(),
                });
                let pair_type = match (&left, &right) {
                    (Some(_), Some(_)) => "modified",
                    (Some(_), None) => "deleted",
                    (None, Some(_)) => "added",
                    (None, None) => unreachable!(),
                };
                hunk.pairs.push(DiffPair {
                    left,
                    right,
                    pair_type: pair_type.to_string(),
                });
            }
            dels.clear();
            adds.clear();
        };

    for line in raw.lines() {
        // Skip file headers
        if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("new file")
            || line.starts_with("deleted file")
        {
            continue;
        }

        if line.starts_with("@@") {
            // Flush previous hunk
            if let Some(ref mut hunk) = current_hunk {
                flush_pending(&mut pending_dels, &mut pending_adds, hunk);
                hunks.push(hunk.clone());
            }

            // Parse @@ -old_start,count +new_start,count @@
            if let Some(rest) = line.strip_prefix("@@ ") {
                let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                if parts.len() >= 2 {
                    if let Some(old_spec) = parts[0].strip_prefix('-') {
                        old_num = old_spec
                            .split(',')
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(1);
                    }
                    if let Some(new_spec) = parts[1].strip_prefix('+') {
                        let clean = new_spec.split("@@").next().unwrap_or(new_spec);
                        new_num = clean
                            .split(',')
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(1);
                    }
                }
            }

            current_hunk = Some(DiffHunk {
                header: line.to_string(),
                pairs: Vec::new(),
            });
            continue;
        }

        let hunk = match current_hunk.as_mut() {
            Some(h) => h,
            None => continue,
        };

        if let Some(content) = line.strip_prefix('+') {
            pending_adds.push((new_num, content.to_string()));
            new_num += 1;
            additions += 1;
        } else if let Some(content) = line.strip_prefix('-') {
            pending_dels.push((old_num, content.to_string()));
            old_num += 1;
            deletions += 1;
        } else {
            // Context line — flush any pending adds/dels first
            flush_pending(&mut pending_dels, &mut pending_adds, hunk);

            let content = line.strip_prefix(' ').unwrap_or(line);
            hunk.pairs.push(DiffPair {
                left: Some(DiffSide {
                    line_num: old_num,
                    content: content.to_string(),
                }),
                right: Some(DiffSide {
                    line_num: new_num,
                    content: content.to_string(),
                }),
                pair_type: "context".to_string(),
            });
            old_num += 1;
            new_num += 1;
        }
    }

    // Flush last hunk
    if let Some(ref mut hunk) = current_hunk {
        flush_pending(&mut pending_dels, &mut pending_adds, hunk);
        hunks.push(hunk.clone());
    }

    SideBySideDiff {
        left_title: left_title.to_string(),
        right_title: right_title.to_string(),
        file_path: file_path.to_string(),
        hunks,
        stats: DiffStats {
            additions,
            deletions,
        },
    }
}

fn parse_conflict_markers(content: &str) -> Vec<ConflictRegion> {
    let mut regions = Vec::new();
    let mut common_lines: Vec<String> = Vec::new();
    let mut ours_lines: Vec<String> = Vec::new();
    let mut theirs_lines: Vec<String> = Vec::new();
    let mut in_ours = false;
    let mut in_theirs = false;

    for line in content.lines() {
        if line.starts_with("<<<<<<<") {
            if !common_lines.is_empty() {
                regions.push(ConflictRegion {
                    region_type: "common".to_string(),
                    ours_lines: common_lines.clone(),
                    theirs_lines: common_lines.clone(),
                    base_lines: common_lines.clone(),
                });
                common_lines.clear();
            }
            in_ours = true;
            in_theirs = false;
        } else if line.starts_with("=======") {
            in_ours = false;
            in_theirs = true;
        } else if line.starts_with(">>>>>>>") {
            in_theirs = false;
            regions.push(ConflictRegion {
                region_type: "conflict".to_string(),
                ours_lines: ours_lines.clone(),
                theirs_lines: theirs_lines.clone(),
                base_lines: Vec::new(),
            });
            ours_lines.clear();
            theirs_lines.clear();
        } else if in_ours {
            ours_lines.push(line.to_string());
        } else if in_theirs {
            theirs_lines.push(line.to_string());
        } else {
            common_lines.push(line.to_string());
        }
    }

    if !common_lines.is_empty() {
        regions.push(ConflictRegion {
            region_type: "common".to_string(),
            ours_lines: common_lines.clone(),
            theirs_lines: common_lines.clone(),
            base_lines: common_lines,
        });
    }

    regions
}

// ── Legacy types (backward compat) ─────────────────────

#[derive(Serialize, Clone)]
pub struct LegacyDiffLine {
    pub content: String,
    pub line_type: String,
}

fn parse_legacy_diff_lines(output: &str) -> Vec<LegacyDiffLine> {
    output
        .lines()
        .map(|line| {
            let line_type = if line.starts_with('+') && !line.starts_with("+++") {
                "add"
            } else if line.starts_with('-') && !line.starts_with("---") {
                "del"
            } else if line.starts_with("@@") {
                "hunk"
            } else if line.starts_with("diff ")
                || line.starts_with("index ")
                || line.starts_with("---")
                || line.starts_with("+++")
            {
                "header"
            } else {
                "context"
            };
            LegacyDiffLine {
                content: line.to_string(),
                line_type: line_type.to_string(),
            }
        })
        .collect()
}
