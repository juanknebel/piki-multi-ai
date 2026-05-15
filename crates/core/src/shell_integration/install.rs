//! Materialize the embedded init scripts to disk and produce the env vars +
//! extra CLI args that, when merged into a `portable_pty::CommandBuilder`,
//! cause the shell to load piki's shell integration on startup.
//!
//! `zsh`, `bash`, and `fish` are supported. Other shells (sh, dash, etc.)
//! return `None` and the caller spawns them as before — without integration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCRIPT_ZSH: &str = include_str!("scripts/integration.zsh");
const SCRIPT_BASH: &str = include_str!("scripts/integration.bash");
const SCRIPT_FISH: &str = include_str!("scripts/integration.fish");

/// The shell families piki knows how to inject integration into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFamily {
    Zsh,
    Bash,
    Fish,
}

impl ShellFamily {
    /// Detect from a shell binary path (e.g. `/bin/zsh`, `/usr/local/bin/bash`).
    /// Returns `None` for shells we don't support.
    pub fn detect(shell_path: &str) -> Option<Self> {
        let basename = Path::new(shell_path).file_name()?.to_str()?;
        match basename {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }
}

/// Env vars + extra args to merge into a `CommandBuilder` so the shell sources
/// piki's integration script on startup.
#[derive(Debug, Default, Clone)]
pub struct IntegrationSetup {
    pub env: HashMap<String, String>,
    /// Args to **prepend** to the command's normal args.
    pub extra_args: Vec<String>,
}

/// Materialize scripts under `base_dir` (idempotent — overwrites existing files
/// with the embedded contents to keep them in sync with the binary version)
/// and return the env/args needed to make the shell pick them up.
///
/// `base_dir` should be a stable per-user location (e.g. `<data_dir>/shell_integration`).
/// Returns `None` for unsupported shells; callers spawn those without
/// integration.
pub fn setup_for(shell_path: &str, base_dir: &Path) -> std::io::Result<Option<IntegrationSetup>> {
    let Some(family) = ShellFamily::detect(shell_path) else {
        return Ok(None);
    };
    materialize(base_dir, family)?;
    let mut setup = IntegrationSetup::default();
    setup
        .env
        .insert("PIKI_SHELL_INTEGRATION".to_string(), "1".to_string());
    match family {
        ShellFamily::Zsh => {
            // zsh sources `$ZDOTDIR/.zshrc` — point it at our bridge dir so
            // our integration runs before the user's real ~/.zshrc.
            setup
                .env
                .insert("ZDOTDIR".to_string(), zsh_bridge_dir(base_dir).display().to_string());
        }
        ShellFamily::Bash => {
            // bash with --rcfile bypasses ~/.bashrc; the bridge file sources
            // our integration and then chains to ~/.bashrc.
            setup.extra_args.push("--rcfile".to_string());
            setup
                .extra_args
                .push(bash_bridge_path(base_dir).display().to_string());
        }
        ShellFamily::Fish => {
            // fish: `-C 'source <path>'` runs after user's config.fish so
            // event handlers stack on top of any user setup. Single-quote the
            // path to handle spaces — fish single quotes are fully literal.
            setup.extra_args.push("-C".to_string());
            setup.extra_args.push(format!(
                "source '{}'",
                fish_integration_path(base_dir).display()
            ));
        }
    }
    Ok(Some(setup))
}

fn materialize(base_dir: &Path, family: ShellFamily) -> std::io::Result<()> {
    std::fs::create_dir_all(base_dir)?;
    match family {
        ShellFamily::Zsh => {
            let bridge = zsh_bridge_dir(base_dir);
            std::fs::create_dir_all(&bridge)?;
            std::fs::write(bridge.join("integration.zsh"), SCRIPT_ZSH)?;
            // Bridge .zshrc: source our script then chain to user's real one.
            // ZDOTDIR override means zsh would otherwise *not* read ~/.zshrc.
            let bridge_zshrc = "# piki-multi shell-integration bridge — auto-generated, do not edit.\n\
                 source \"$ZDOTDIR/integration.zsh\"\n\
                 if [ -f \"$HOME/.zshrc\" ]; then\n\
                 \x20\x20ZDOTDIR=\"$HOME\" source \"$HOME/.zshrc\"\n\
                 fi\n";
            std::fs::write(bridge.join(".zshrc"), bridge_zshrc)?;
        }
        ShellFamily::Bash => {
            std::fs::write(base_dir.join("integration.bash"), SCRIPT_BASH)?;
            let bridge = bash_bridge_path(base_dir);
            let bridge_rc = format!(
                "# piki-multi shell-integration bridge — auto-generated, do not edit.\n\
                 source \"{}/integration.bash\"\n\
                 if [ -f \"$HOME/.bashrc\" ]; then\n\
                 \x20\x20source \"$HOME/.bashrc\"\n\
                 fi\n",
                base_dir.display()
            );
            std::fs::write(&bridge, bridge_rc)?;
        }
        ShellFamily::Fish => {
            // No bridge file needed — fish's `-C 'source <path>'` runs after
            // config.fish already sourced the user's setup.
            std::fs::write(fish_integration_path(base_dir), SCRIPT_FISH)?;
        }
    }
    Ok(())
}

fn zsh_bridge_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("zsh-bridge")
}

fn bash_bridge_path(base_dir: &Path) -> PathBuf {
    base_dir.join("bash-bridge.sh")
}

fn fish_integration_path(base_dir: &Path) -> PathBuf {
    base_dir.join("integration.fish")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_known_shells() {
        assert_eq!(ShellFamily::detect("/bin/zsh"), Some(ShellFamily::Zsh));
        assert_eq!(
            ShellFamily::detect("/usr/local/bin/bash"),
            Some(ShellFamily::Bash)
        );
        assert_eq!(ShellFamily::detect("/usr/bin/fish"), Some(ShellFamily::Fish));
        assert_eq!(ShellFamily::detect("zsh"), Some(ShellFamily::Zsh));
        assert_eq!(ShellFamily::detect("fish"), Some(ShellFamily::Fish));
    }

    #[test]
    fn detect_unknown_shells_returns_none() {
        assert_eq!(ShellFamily::detect("/bin/sh"), None);
        assert_eq!(ShellFamily::detect("/bin/dash"), None);
    }

    #[test]
    fn setup_zsh_writes_bridge_and_returns_zdotdir() {
        let dir = tempfile::tempdir().unwrap();
        let setup = setup_for("/bin/zsh", dir.path()).unwrap().unwrap();
        let zdotdir = setup.env.get("ZDOTDIR").expect("ZDOTDIR set");
        assert!(setup.extra_args.is_empty());
        assert_eq!(setup.env.get("PIKI_SHELL_INTEGRATION").unwrap(), "1");
        assert!(Path::new(zdotdir).join(".zshrc").exists());
        assert!(Path::new(zdotdir).join("integration.zsh").exists());
    }

    #[test]
    fn setup_bash_writes_bridge_and_returns_rcfile_args() {
        let dir = tempfile::tempdir().unwrap();
        let setup = setup_for("/bin/bash", dir.path()).unwrap().unwrap();
        assert_eq!(setup.env.get("PIKI_SHELL_INTEGRATION").unwrap(), "1");
        assert!(!setup.env.contains_key("ZDOTDIR"));
        assert_eq!(setup.extra_args.len(), 2);
        assert_eq!(setup.extra_args[0], "--rcfile");
        assert!(Path::new(&setup.extra_args[1]).exists());
        assert!(dir.path().join("integration.bash").exists());
    }

    #[test]
    fn setup_fish_writes_script_and_returns_init_command_args() {
        let dir = tempfile::tempdir().unwrap();
        let setup = setup_for("/usr/bin/fish", dir.path()).unwrap().unwrap();
        assert_eq!(setup.env.get("PIKI_SHELL_INTEGRATION").unwrap(), "1");
        assert!(!setup.env.contains_key("ZDOTDIR"));
        assert_eq!(setup.extra_args.len(), 2);
        assert_eq!(setup.extra_args[0], "-C");
        assert!(setup.extra_args[1].starts_with("source '"));
        assert!(setup.extra_args[1].contains("integration.fish"));
        assert!(dir.path().join("integration.fish").exists());
    }

    #[test]
    fn setup_unsupported_shell_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let setup = setup_for("/bin/sh", dir.path()).unwrap();
        assert!(setup.is_none());
    }

    #[test]
    fn materialize_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        setup_for("/bin/zsh", dir.path()).unwrap().unwrap();
        // Touch the file to a different size; setup should overwrite back.
        let zshrc = zsh_bridge_dir(dir.path()).join("integration.zsh");
        std::fs::write(&zshrc, b"corrupted").unwrap();
        setup_for("/bin/zsh", dir.path()).unwrap().unwrap();
        let contents = std::fs::read_to_string(&zshrc).unwrap();
        assert!(contents.contains("__piki_osc_prompt_start"));
    }
}
