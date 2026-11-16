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

use crate::providers::{ProviderConfig, ProviderManager};
use crate::storage::AgentProfile;

/// Conventional agent directory per agent CLI, keyed by the provider's
/// command basename — the fallback for a provider whose `agent_dir` the user
/// never filled in (the seeded Claude entry has one; a provider added through
/// the dialog usually doesn't). Matched the same way
/// [`crate::cli_agent::bridge_for_command`] and
/// `agent_state_detect::manifest_for_command` match theirs, so "piki knows
/// this agent" means the same thing everywhere.
///
/// `agy` (Antigravity) reads Gemini's tree — its plugin bridge is installed
/// under `~/.gemini/` — so it shares `.gemini/agents`.
const DEFAULT_AGENT_DIRS: &[(&str, &str)] = &[
    ("claude", ".claude/agents"),
    ("gemini", ".gemini/agents"),
    ("agy", ".gemini/agents"),
    ("codex", ".codex/agents"),
    ("muse", ".muse/agents"),
    ("opencode", ".opencode/agents"),
    ("kilo", ".kilo/agents"),
];

/// The conventional agent directory for a command (`/usr/local/bin/codex`
/// matches `codex`), or `None` for a CLI piki knows nothing about.
pub fn default_agent_dir(command: &str) -> Option<&'static str> {
    let base = Path::new(command)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())?;
    DEFAULT_AGENT_DIRS
        .iter()
        .find(|(cmd, _)| *cmd == base)
        .map(|(_, dir)| *dir)
}

/// Where this provider's agent files live, relative to a checkout: what the
/// user configured, else the convention for its command. `None` when neither
/// is known — nothing to scan, and nothing to sync into.
pub fn agent_dir_for(config: &ProviderConfig) -> Option<String> {
    config
        .agent_dir
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(String::from)
        .or_else(|| default_agent_dir(&config.command).map(String::from))
}

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

/// Scan a checkout for agent definition files.
///
/// `root` is the workspace the user is standing in, NOT its `source_repo`: a
/// worktree is its own checkout, and an agent file written (or not yet
/// committed) there does not exist in the parent repo's working tree. One
/// directory per provider ([`agent_dir_for`]); results are sorted by
/// (provider, name) so the import list doesn't reshuffle between runs on the
/// whims of readdir order.
pub fn scan_repo_agents(
    root: &Path,
    providers: &ProviderManager,
    existing: &[AgentProfile],
) -> Vec<ScannedAgent> {
    let mut found = Vec::new();

    for config in providers.all() {
        let Some(agent_dir) = agent_dir_for(config) else {
            continue;
        };
        let dir = root.join(agent_dir);
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
        provider_cmd(name, &name.to_lowercase(), agent_dir)
    }

    /// Same, with the command spelled out — the fallback keys off it.
    fn provider_cmd(name: &str, command: &str, agent_dir: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            name: name.to_string(),
            description: String::new(),
            command: command.to_string(),
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

    /// A provider the user added through the dialog usually has no
    /// `agent_dir` — Codex, Muse and Antigravity all ship that way — and used
    /// to be skipped outright, so their agents were invisible to the import.
    #[test]
    fn a_provider_without_an_agent_dir_falls_back_to_its_convention() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".codex/agents/reviewer.md", "x");
        write(tmp.path(), ".muse/agents/planner.md", "y");
        write(tmp.path(), ".gemini/agents/scout.md", "z");

        let mgr = manager(vec![
            provider_cmd("Codex", "codex", None),
            provider_cmd("Muse", "muse", None),
            // Antigravity reads Gemini's tree.
            provider_cmd("Antigravity", "agy", None),
        ]);
        let found = scan_repo_agents(tmp.path(), &mgr, &[]);

        let by_provider: Vec<(&str, &str)> = found
            .iter()
            .map(|a| (a.provider.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(
            by_provider,
            vec![
                ("Antigravity", "scout"),
                ("Codex", "reviewer"),
                ("Muse", "planner"),
            ]
        );
    }

    /// The configured directory always wins over the convention.
    #[test]
    fn a_configured_agent_dir_beats_the_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".codex/agents/conventional.md", "x");
        write(tmp.path(), "agents/mine.md", "y");

        let mgr = manager(vec![provider_cmd("Codex", "codex", Some("agents"))]);
        let found = scan_repo_agents(tmp.path(), &mgr, &[]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "mine");
    }

    #[test]
    fn an_unknown_command_with_no_agent_dir_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".whatever/agents/x.md", "x");
        let mgr = manager(vec![provider_cmd("Mystery", "mystery-cli", None)]);
        assert!(scan_repo_agents(tmp.path(), &mgr, &[]).is_empty());
    }

    #[test]
    fn default_dirs_match_on_the_command_basename() {
        assert_eq!(default_agent_dir("claude"), Some(".claude/agents"));
        assert_eq!(
            default_agent_dir("/usr/local/bin/codex"),
            Some(".codex/agents")
        );
        assert_eq!(default_agent_dir("CODEX"), Some(".codex/agents"));
        assert_eq!(default_agent_dir("agy"), Some(".gemini/agents"));
        assert_eq!(default_agent_dir("mystery-cli"), None);
        assert_eq!(default_agent_dir(""), None);
    }

    /// An empty string in the file is "not configured", not a dir named "".
    #[test]
    fn a_blank_agent_dir_is_treated_as_unset() {
        let cfg = provider_cmd("Codex", "codex", Some("   "));
        assert_eq!(agent_dir_for(&cfg).as_deref(), Some(".codex/agents"));
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
