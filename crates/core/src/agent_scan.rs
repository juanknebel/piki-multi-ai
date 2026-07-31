//! Discovering agent definition files (`<agent_dir>/*.md`) inside a repo.
//!
//! Both frontends offer "import the agents this repo ships with", and both
//! used to implement the walk themselves. They disagreed on all three of the
//! decisions involved:
//!
//! * **Which directories to scan.** The TUI derived them from the configured
//!   providers; the desktop hardcoded five (`.claude/agents`, `.gemini/agents`,
//!   `.opencode/agents`, `.kilo/agents`, `.codex/agents`) and appended the
//!   configured ones. So each found agents the other missed.
//! * **What provider to attribute them to.** The desktop's hardcoded labels
//!   ("Claude Code", "Gemini", …) need not match any provider the user
//!   actually has, which imports an agent pointing at a provider that doesn't
//!   exist.
//! * **Whether an agent is already imported.** The TUI matched on name *and*
//!   provider; the desktop on name alone, so a same-named agent under a
//!   different provider was silently treated as already-imported and skipped.
//!
//! `scan_repo_agents` is the single answer: directories come from the
//! configured providers, the label is that provider's real name, and
//! "already imported" means the (name, provider) pair matches.

use std::path::Path;

use crate::providers::ProviderManager;
use crate::storage::AgentProfile;

/// An agent definition file found in the repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedAgent {
    /// File stem — `code-reviewer.md` becomes `code-reviewer`.
    pub name: String,
    /// Name of the provider whose `agent_dir` it was found under.
    pub provider: String,
    /// File contents.
    pub role: String,
    /// An agent with this exact (name, provider) is already stored.
    pub exists: bool,
}

/// Scan `source_repo` for agent definition files.
///
/// One directory per provider that configures an `agent_dir`; results are
/// sorted by (provider, name) so the import list doesn't reshuffle between
/// runs on the whims of readdir order.
pub fn scan_repo_agents(
    source_repo: &Path,
    providers: &ProviderManager,
    existing: &[AgentProfile],
) -> Vec<ScannedAgent> {
    let mut found = Vec::new();

    for config in providers.all() {
        let Some(ref agent_dir) = config.agent_dir else {
            continue;
        };
        let dir = source_repo.join(agent_dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue; // missing directory is the normal case, not an error
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let Some(name) = path.file_stem().map(|s| s.to_string_lossy().to_string()) else {
                continue;
            };
            let role = std::fs::read_to_string(&path).unwrap_or_default();
            // Name AND provider: the same agent name under two providers is
            // two different agents.
            let exists = existing
                .iter()
                .any(|a| a.name == name && a.provider == config.name);
            found.push(ScannedAgent {
                name,
                provider: config.name.clone(),
                role,
                exists,
            });
        }
    }

    found.sort_by(|a, b| (&a.provider, &a.name).cmp(&(&b.provider, &b.name)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{PromptFormat, ProviderConfig};

    fn provider(name: &str, agent_dir: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            name: name.to_string(),
            description: String::new(),
            command: name.to_lowercase(),
            default_args: Vec::new(),
            prompt_format: PromptFormat::Positional,
            dispatchable: true,
            agent_dir: agent_dir.map(String::from),
            idle_threshold_secs: None,
            idle_notify: false,
            icon: None,
        }
    }

    fn manager(configs: Vec<ProviderConfig>) -> ProviderManager {
        let mut m = ProviderManager::empty();
        for c in configs {
            m.upsert(c);
        }
        m
    }

    fn profile(name: &str, provider: &str) -> AgentProfile {
        AgentProfile {
            id: None,
            source_repo: String::new(),
            name: name.to_string(),
            provider: provider.to_string(),
            role: String::new(),
            version: 1,
            last_synced_at: None,
        }
    }

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn finds_md_files_under_each_configured_agent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".claude/agents/reviewer.md", "review things");
        write(tmp.path(), ".gemini/agents/planner.md", "plan things");

        let mgr = manager(vec![
            provider("Claude", Some(".claude/agents")),
            provider("Gemini", Some(".gemini/agents")),
        ]);
        let found = scan_repo_agents(tmp.path(), &mgr, &[]);

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "reviewer");
        assert_eq!(found[0].provider, "Claude");
        assert_eq!(found[0].role, "review things");
        assert_eq!(found[1].provider, "Gemini");
    }

    #[test]
    fn ignores_non_markdown_and_missing_directories() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".claude/agents/notes.txt", "not an agent");
        write(tmp.path(), ".claude/agents/real.md", "an agent");

        let mgr = manager(vec![
            provider("Claude", Some(".claude/agents")),
            // Configured but the directory doesn't exist — must not error.
            provider("Ghost", Some(".ghost/agents")),
        ]);
        let found = scan_repo_agents(tmp.path(), &mgr, &[]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "real");
    }

    #[test]
    fn providers_without_an_agent_dir_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".claude/agents/reviewer.md", "x");
        let mgr = manager(vec![provider("Claude", None)]);
        assert!(scan_repo_agents(tmp.path(), &mgr, &[]).is_empty());
    }

    /// The desktop compared names only, so an agent named the same under a
    /// different provider was wrongly reported as already imported — and
    /// therefore never got imported.
    #[test]
    fn exists_matches_on_name_and_provider_together() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".claude/agents/reviewer.md", "x");
        write(tmp.path(), ".gemini/agents/reviewer.md", "y");

        let mgr = manager(vec![
            provider("Claude", Some(".claude/agents")),
            provider("Gemini", Some(".gemini/agents")),
        ]);
        // Only the Claude one is stored.
        let found = scan_repo_agents(tmp.path(), &mgr, &[profile("reviewer", "Claude")]);

        assert_eq!(found.len(), 2);
        let claude = found.iter().find(|a| a.provider == "Claude").unwrap();
        let gemini = found.iter().find(|a| a.provider == "Gemini").unwrap();
        assert!(claude.exists);
        assert!(
            !gemini.exists,
            "same name under another provider is a different agent"
        );
    }

    #[test]
    fn results_are_sorted_so_the_import_list_is_stable() {
        let tmp = tempfile::tempdir().unwrap();
        for n in ["zeta", "alpha", "mid"] {
            write(tmp.path(), &format!(".claude/agents/{n}.md"), "x");
        }
        let mgr = manager(vec![provider("Claude", Some(".claude/agents"))]);
        let names: Vec<String> = scan_repo_agents(tmp.path(), &mgr, &[])
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, ["alpha", "mid", "zeta"]);
    }

    #[test]
    fn empty_repo_yields_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = manager(vec![provider("Claude", Some(".claude/agents"))]);
        assert!(scan_repo_agents(tmp.path(), &mgr, &[]).is_empty());
    }
}
