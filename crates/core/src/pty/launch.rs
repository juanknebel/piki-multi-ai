//! Everything needed to start a tab's process, resolved in one place.
//!
//! The TUI and the desktop app both have to answer the same questions before
//! they can spawn a PTY: which binary, which arguments, which environment,
//! does this tab get shell integration, does it get a structured cli-agent
//! channel. They used to answer them separately, and drifted — the desktop
//! never learned about passive agent-state detection (Codex showed no live
//! status there) and never wired the cli-agent FIFO into shell tabs (a
//! manually-typed `claude` never reached its Agents list), while the TUI never
//! learned about the user's configured shell. Both now call [`launch_plan`],
//! so a new capability lands in both at once.

use std::path::PathBuf;

use crate::AIProvider;
use crate::cli_agent::install as cli_agent_install;
use crate::cli_agent::install_antigravity as agy_install;
use crate::cli_agent::{AgentBridge, bridge_for_command};
use crate::paths::DataPaths;
use crate::providers::ProviderManager;
use crate::shell_integration::install as shell_install;

/// A resolved spawn: hand the fields straight to `PtySession::spawn` (TUI) or
/// `RawPtySession::spawn` (desktop).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LaunchPlan {
    /// Binary to execute (resolved through `$PATH` by the spawner).
    pub command: String,
    /// Arguments for the command, including any prompt arguments.
    pub args: Vec<String>,
    /// Extra environment for the child process.
    pub env: Vec<(String, String)>,
    /// Arguments contributed by shell integration / a hook bridge, appended
    /// after `args` by the caller. Kept separate because they belong to the
    /// wrapper, not to the user's provider configuration.
    pub extra_args: Vec<String>,
    /// Run the OSC parser over this tab's output. True for shell tabs, for
    /// hook-bridge agents, and for agents we can only watch passively.
    pub integration_on: bool,
    /// Per-spawn FIFO for the structured cli-agent channel. `None` when this
    /// tab has no hook bridge — passive detection must not get one.
    pub cli_agent_sock: Option<PathBuf>,
}

/// Why a tab cannot be launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// Renders from application state and never spawns a process
    /// (Kanban, Api, CodeReview).
    NotATerminal(AIProvider),
    /// `AIProvider::Custom(name)` with no matching `providers.toml` entry.
    UnknownProvider(String),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotATerminal(p) => {
                write!(f, "{} does not use a terminal session", p.label())
            }
            Self::UnknownProvider(name) => {
                write!(f, "No provider configured named '{name}'")
            }
        }
    }
}

impl std::error::Error for LaunchError {}

/// Resolve how to launch `provider`.
///
/// `prompt` is the initial task text for a dispatched agent, turned into
/// arguments by the provider's `prompt_format`. `shell_override` is the user's
/// configured shell, used only for [`AIProvider::Shell`]; `None` falls back to
/// `$SHELL`.
pub fn launch_plan(
    provider: &AIProvider,
    prompt: Option<&str>,
    provider_manager: Option<&ProviderManager>,
    paths: &DataPaths,
    shell_override: Option<&str>,
) -> Result<LaunchPlan, LaunchError> {
    let (command, args) = resolve_command(provider, prompt, provider_manager, shell_override)?;
    if command.is_empty() {
        return Err(LaunchError::NotATerminal(provider.clone()));
    }

    let integration = resolve_integration(provider, &command, paths);
    Ok(LaunchPlan {
        command,
        args,
        env: integration.env,
        extra_args: integration.extra_args,
        integration_on: integration.integration_on,
        cli_agent_sock: integration.cli_agent_sock,
    })
}

fn resolve_command(
    provider: &AIProvider,
    prompt: Option<&str>,
    provider_manager: Option<&ProviderManager>,
    shell_override: Option<&str>,
) -> Result<(String, Vec<String>), LaunchError> {
    // Providers that render from app state have no command at all.
    if matches!(
        provider,
        AIProvider::Kanban | AIProvider::Api | AIProvider::CodeReview
    ) {
        return Err(LaunchError::NotATerminal(provider.clone()));
    }

    match provider {
        AIProvider::Shell => {
            let command = shell_override
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| provider.resolved_command());
            Ok((command, Vec::new()))
        }
        AIProvider::Custom(name) => {
            let config = provider_manager
                .and_then(|m| m.get(name))
                .ok_or_else(|| LaunchError::UnknownProvider(name.clone()))?;
            let mut args = config.default_args.clone();
            if let Some(p) = prompt {
                args.extend(ProviderManager::prompt_args(config, p));
            }
            Ok((config.command.clone(), args))
        }
        _ => {
            let args = prompt.map(|p| provider.prompt_args(p)).unwrap_or_default();
            Ok((provider.resolved_command(), args))
        }
    }
}

struct Integration {
    env: Vec<(String, String)>,
    extra_args: Vec<String>,
    integration_on: bool,
    cli_agent_sock: Option<PathBuf>,
}

impl Integration {
    fn bare() -> Self {
        Self {
            env: Vec::new(),
            extra_args: Vec::new(),
            integration_on: false,
            cli_agent_sock: None,
        }
    }
}

/// Shell tabs get OSC 133/7 shell integration. Provider tabs whose binary has
/// a hook bridge (Claude Code, Antigravity) get the structured cli-agent
/// channel. Both ride the same OSC parser, so both enable `integration_on`.
/// Agents we can only watch passively (Codex) get the parser but no FIFO.
/// Everything else runs bare.
///
/// Every failure here degrades rather than aborts: the tab still spawns, it
/// just loses rich status.
fn resolve_integration(provider: &AIProvider, command: &str, paths: &DataPaths) -> Integration {
    if *provider == AIProvider::Shell {
        return shell_integration(command, paths);
    }

    let bridge = match provider {
        AIProvider::Custom(_) => bridge_for_command(command),
        _ => None,
    };

    match bridge {
        Some(AgentBridge::Claude) => {
            match cli_agent_install::setup_for_claude(&paths.claude_hooks_dir()) {
                Ok(setup) => Integration {
                    cli_agent_sock: setup.sock_path.clone(),
                    env: setup.env.into_iter().collect(),
                    extra_args: setup.extra_args,
                    integration_on: true,
                },
                Err(e) => {
                    tracing::warn!(error = %e, "claude cli-agent hook setup failed");
                    Integration::bare()
                }
            }
        }
        Some(AgentBridge::Antigravity) => {
            // No extra_args: agy discovers the bridge from its own plugins
            // root, so the hooks ride the environment alone.
            match agy_install::setup_for_antigravity(
                &paths.antigravity_hooks_dir(),
                &agy_install::plugins_root(),
            ) {
                Ok(setup) => Integration {
                    cli_agent_sock: setup.sock_path.clone(),
                    env: setup.env.into_iter().collect(),
                    extra_args: Vec::new(),
                    integration_on: true,
                },
                Err(e) => {
                    tracing::warn!(error = %e, "antigravity cli-agent hook setup failed");
                    Integration::bare()
                }
            }
        }
        None if crate::agent_state_detect::manifest_for_command(command).is_some() => {
            // No hook bridge for this provider (e.g. Codex) — turn on shell
            // integration so the OSC parser captures its window-title
            // spinner, but withhold `cli_agent_sock`: that FIFO is exclusive
            // to the real hook bridges above.
            Integration {
                integration_on: true,
                ..Integration::bare()
            }
        }
        None => Integration::bare(),
    }
}

fn shell_integration(command: &str, paths: &DataPaths) -> Integration {
    let setup = match shell_install::setup_for(command, &paths.shell_integration_dir()) {
        Ok(Some(setup)) => setup,
        Ok(None) => return Integration::bare(),
        Err(e) => {
            tracing::warn!(error = %e, shell = %command, "shell integration setup failed");
            return Integration::bare();
        }
    };

    let mut env: Vec<(String, String)> = setup.env.into_iter().collect();
    // Also wire the cli-agent channel so a manually-typed `claude` inside this
    // shell reports to the Agents list: the FIFO + hook env ride the shell's
    // environment, and the bridge script wraps `claude` with `--settings`.
    // Only the env is merged — the `--settings` extra_args are claude's
    // arguments, not the shell's.
    let sock = match cli_agent_install::setup_for_claude(&paths.claude_hooks_dir()) {
        Ok(agent) => {
            env.extend(agent.env);
            agent.sock_path
        }
        Err(e) => {
            tracing::debug!(error = %e, "cli-agent channel skipped for shell tab");
            None
        }
    };

    Integration {
        env,
        extra_args: setup.extra_args,
        integration_on: true,
        cli_agent_sock: sock,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{PromptFormat, ProviderConfig};

    fn paths() -> DataPaths {
        DataPaths::new(std::env::temp_dir().join("piki-launch-plan-tests"))
    }

    fn provider_cfg(name: &str, command: &str) -> ProviderConfig {
        ProviderConfig {
            name: name.to_string(),
            description: String::new(),
            command: command.to_string(),
            default_args: vec!["--flag".to_string()],
            prompt_format: PromptFormat::Positional,
            dispatchable: true,
            agent_dir: None,
            idle_threshold_secs: None,
            idle_notify: false,
            icon: None,
        }
    }

    fn manager(configs: &[ProviderConfig]) -> ProviderManager {
        let mut m = ProviderManager::empty();
        for c in configs {
            m.upsert(c.clone());
        }
        m
    }

    #[test]
    fn state_rendered_providers_are_not_terminals() {
        for p in [AIProvider::Kanban, AIProvider::Api, AIProvider::CodeReview] {
            let err = launch_plan(&p, None, None, &paths(), None).unwrap_err();
            assert_eq!(err, LaunchError::NotATerminal(p));
        }
    }

    #[test]
    fn unknown_custom_provider_is_named_in_the_error() {
        let err = launch_plan(
            &AIProvider::Custom("ghost".into()),
            None,
            Some(&manager(&[])),
            &paths(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, LaunchError::UnknownProvider("ghost".into()));
        assert!(err.to_string().contains("ghost"));
    }

    #[test]
    fn custom_provider_args_are_default_args_then_prompt_args() {
        let mgr = manager(&[provider_cfg("acme", "acme-cli")]);
        let plan = launch_plan(
            &AIProvider::Custom("acme".into()),
            Some("do the thing"),
            Some(&mgr),
            &paths(),
            None,
        )
        .unwrap();
        assert_eq!(plan.command, "acme-cli");
        assert_eq!(plan.args, vec!["--flag", "do the thing"]);
    }

    #[test]
    fn custom_provider_without_a_prompt_keeps_only_default_args() {
        let mgr = manager(&[provider_cfg("acme", "acme-cli")]);
        let plan = launch_plan(
            &AIProvider::Custom("acme".into()),
            None,
            Some(&mgr),
            &paths(),
            None,
        )
        .unwrap();
        assert_eq!(plan.args, vec!["--flag"]);
    }

    /// The desktop honoured a user-configured shell and the TUI did not.
    #[test]
    fn shell_override_wins_and_blank_falls_back() {
        let plan = launch_plan(&AIProvider::Shell, None, None, &paths(), Some("/bin/zsh")).unwrap();
        assert_eq!(plan.command, "/bin/zsh");

        // Blank/whitespace is treated as "unset", not as a command.
        for blank in ["", "   "] {
            let plan = launch_plan(&AIProvider::Shell, None, None, &paths(), Some(blank)).unwrap();
            assert_eq!(plan.command, AIProvider::Shell.resolved_command());
        }
    }

    /// A provider with no bridge and no detection manifest runs bare — in
    /// particular it must never be handed a cli-agent FIFO.
    #[test]
    fn unknown_binary_runs_without_integration() {
        let mgr = manager(&[provider_cfg("plain", "definitely-not-a-known-agent")]);
        let plan = launch_plan(
            &AIProvider::Custom("plain".into()),
            None,
            Some(&mgr),
            &paths(),
            None,
        )
        .unwrap();
        assert!(!plan.integration_on);
        assert!(plan.cli_agent_sock.is_none());
        assert!(plan.env.is_empty());
    }

    /// Codex has no hook bridge but is detectable passively: the OSC parser
    /// must be on so its window-title spinner is read, while the FIFO stays
    /// reserved for real bridges. This is the branch the desktop was missing.
    #[test]
    fn passively_detected_agent_gets_the_parser_but_no_fifo() {
        // codex is the passive-detection manifest today; assert the wiring
        // through it rather than restating the manifest list here.
        assert!(
            crate::agent_state_detect::manifest_for_command("codex").is_some(),
            "test assumes codex is passively detectable"
        );
        let mgr = manager(&[provider_cfg("passive", "codex")]);
        let plan = launch_plan(
            &AIProvider::Custom("passive".into()),
            None,
            Some(&mgr),
            &paths(),
            None,
        )
        .unwrap();
        assert!(
            plan.integration_on,
            "passive detection needs the OSC parser"
        );
        assert!(
            plan.cli_agent_sock.is_none(),
            "the cli-agent FIFO is exclusive to hook bridges"
        );
    }
}
