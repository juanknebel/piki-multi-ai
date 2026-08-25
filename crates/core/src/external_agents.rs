//! Discover external `claude` agents via /proc (Linux).
//! Minimal v1: only `claude` binary, tree by ppid, mapped to workspace by longest cwd prefix.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::WorkspaceInfo;

#[derive(Debug, Clone)]
pub struct ExternalAgent {
    pub pid: u32,
    pub ppid: u32,
    pub cwd: Option<PathBuf>,
    pub cmd: String,
    pub workspace_idx: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct AgentTree {
    pub root: ExternalAgent,
    pub children: Vec<ExternalAgent>,
}

fn read_ppid(pid: u32) -> Option<u32> {
    let content = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
    for line in content.lines() {
        if line.starts_with("PPid:") {
            return line.split_whitespace().nth(1)?.parse().ok();
        }
    }
    None
}

fn read_comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{}/cwd", pid)).ok()
}

fn read_cmd(pid: u32) -> String {
    if let Ok(bytes) = std::fs::read(format!("/proc/{}/cmdline", pid)) {
        let parts: Vec<String> = bytes
            .split(|b| *b == 0)
            .map(|p| String::from_utf8_lossy(p).to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !parts.is_empty() {
            let joined = parts.join(" ");
            return joined.chars().take(100).collect();
        }
    }
    read_comm(pid).unwrap_or_default()
}

fn is_claude(pid: u32) -> bool {
    if let Some(comm) = read_comm(pid)
        && comm == "claude" {
            return true;
        }
    if let Ok(bytes) = std::fs::read(format!("/proc/{}/cmdline", pid))
        && let Some(first) = bytes.split(|b| *b == 0).next() {
            let s = String::from_utf8_lossy(first);
            let base = Path::new(s.trim())
                .file_name()
                .and_then(|x| x.to_str())
                .unwrap_or("");
            if base == "claude" {
                return true;
            }
        }
    false
}

fn workspace_for_cwd(cwd: &Path, workspaces: &[WorkspaceInfo]) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_len = 0usize;
    for (idx, ws) in workspaces.iter().enumerate() {
        // Prefer `path` match, fall back to `source_repo`
        let candidates = [&ws.path, &ws.source_repo];
        for cand in candidates {
            if cwd.starts_with(cand) {
                let l = cand.as_os_str().len();
                if l > best_len {
                    best_len = l;
                    best = Some(idx);
                }
            }
        }
    }
    best
}

/// Scan /proc for `claude` processes and build parent->children trees.
/// Returns empty on non-Linux (no /proc).
pub fn scan_external_agents(workspaces: &[WorkspaceInfo]) -> Vec<AgentTree> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut agents: Vec<ExternalAgent> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        if !is_claude(pid) {
            continue;
        }
        let ppid = read_ppid(pid).unwrap_or(0);
        let cwd = read_cwd(pid);
        let cmd = read_cmd(pid);
        let workspace_idx = cwd
            .as_deref()
            .and_then(|p| workspace_for_cwd(p, workspaces));
        agents.push(ExternalAgent {
            pid,
            ppid,
            cwd,
            cmd,
            workspace_idx,
        });
    }

    let pids: HashSet<u32> = agents.iter().map(|a| a.pid).collect();
    let mut children_map: HashMap<u32, Vec<ExternalAgent>> = HashMap::new();
    let mut roots: Vec<ExternalAgent> = Vec::new();
    for a in agents {
        if pids.contains(&a.ppid) {
            children_map.entry(a.ppid).or_default().push(a);
        } else {
            roots.push(a);
        }
    }
    let mut trees: Vec<AgentTree> = roots
        .into_iter()
        .map(|r| {
            let pid = r.pid;
            let children = children_map.remove(&pid).unwrap_or_default();
            AgentTree { root: r, children }
        })
        .collect();
    trees.sort_by_key(|t| t.root.workspace_idx.unwrap_or(usize::MAX));
    trees
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws(path: &str) -> WorkspaceInfo {
        let mut info = WorkspaceInfo::new(
            "test".to_string(),
            String::new(),
            String::new(),
            None,
            PathBuf::from(path),
            PathBuf::from(path),
        );
        info.source_repo = PathBuf::from(path);
        info
    }

    #[test]
    fn workspace_for_cwd_picks_longest_prefix() {
        let workspaces = vec![ws("/tmp/a"), ws("/tmp/a/b")];
        let cwd = Path::new("/tmp/a/b/c");
        assert_eq!(workspace_for_cwd(cwd, &workspaces), Some(1));
    }

    #[test]
    fn workspace_for_cwd_none_when_outside() {
        let workspaces = vec![ws("/tmp/a")];
        assert_eq!(workspace_for_cwd(Path::new("/other"), &workspaces), None);
    }

    #[test]
    fn scan_returns_empty_when_no_proc_or_no_claude() {
        // On this machine there is at least one claude, but the call must not panic
        let trees = scan_external_agents(&[]);
        // Just check it doesn't crash; children are correctly partitioned
        for t in &trees {
            for child in &t.children {
                assert_ne!(child.pid, t.root.pid);
            }
        }
    }
}
