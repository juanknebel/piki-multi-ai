use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use piki_core::repo_skills::{self, Skill};

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct SkillOfferDto {
    pub id: String,
    pub title: String,
    pub markdown: String,
}

impl From<&Skill> for SkillOfferDto {
    fn from(s: &Skill) -> Self {
        Self {
            id: s.id.to_string(),
            title: s.title.to_string(),
            markdown: s.markdown.to_string(),
        }
    }
}

/// Resolve a workspace's `(source_repo, AGENTS.md path)` under the lock,
/// cloning out so the `storage` calls below don't need the lock held.
fn resolve_target(
    app: &DesktopApp,
    workspace_idx: usize,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let ws = app
        .workspaces
        .get(workspace_idx)
        .ok_or("Workspace index out of range")?;
    Ok((ws.info.source_repo.clone(), ws.info.path.join("AGENTS.md")))
}

/// Called right after a workspace is added/created: silently (re-)apply
/// whatever skills were already accepted for this `source_repo` into this
/// checkout's `AGENTS.md` (covers a second worktree of an already-decided
/// repo), then return the skills still undecided — what a "add piki skills
/// to AGENTS.md?" prompt should offer. Empty means nothing to ask.
#[tauri::command]
pub async fn sync_and_list_pending_skill_offers(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<SkillOfferDto>, String> {
    let (source_repo, agents_md_path) = {
        let app = state.lock();
        resolve_target(&app, workspace_idx)?
    };
    let storage = {
        let app = state.lock();
        app.storage.clone()
    };
    let Some(repo_skills_storage) = storage.repo_skills.as_deref() else {
        return Ok(Vec::new());
    };

    repo_skills::sync_accepted_skills(repo_skills_storage, &source_repo, &agents_md_path)
        .map_err(|e| e.to_string())?;

    Ok(
        repo_skills::pending_offers(repo_skills_storage, &source_repo)
            .into_iter()
            .map(SkillOfferDto::from)
            .collect(),
    )
}

/// Record the user's decision on each listed skill id for this workspace's
/// `source_repo`, then resync the injected `AGENTS.md` block from the full
/// accepted set (so a decline removes it, an accept adds it, alongside
/// whatever else was already accepted).
#[tauri::command]
pub async fn respond_to_skill_offers(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    accepted: Vec<String>,
    declined: Vec<String>,
) -> Result<(), String> {
    let (source_repo, agents_md_path) = {
        let app = state.lock();
        resolve_target(&app, workspace_idx)?
    };
    let storage = {
        let app = state.lock();
        app.storage.clone()
    };
    let Some(repo_skills_storage) = storage.repo_skills.as_deref() else {
        return Err("Skill storage is unavailable".to_string());
    };

    for id in &accepted {
        if let Some(skill) = repo_skills::SKILLS.iter().find(|s| s.id == *id) {
            repo_skills_storage
                .set_skill_offer(&source_repo, skill.id, skill.version, "accepted")
                .map_err(|e| e.to_string())?;
        }
    }
    for id in &declined {
        if let Some(skill) = repo_skills::SKILLS.iter().find(|s| s.id == *id) {
            repo_skills_storage
                .set_skill_offer(&source_repo, skill.id, skill.version, "declined")
                .map_err(|e| e.to_string())?;
        }
    }

    repo_skills::sync_accepted_skills(repo_skills_storage, &source_repo, &agents_md_path)
        .map_err(|e| e.to_string())
}
