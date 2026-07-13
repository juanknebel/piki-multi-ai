//! Materialize the Antigravity (`agy`) hook bridge and produce the env that
//! makes the hooks report back to piki.
//!
//! Unlike Claude Code — which takes a `--settings <file>` argument, so piki can
//! hand it a per-spawn hook config and touch nothing else — `agy` has no
//! per-spawn hook flag. Its lifecycle hooks are only discovered from a
//! **plugin**: a directory under the user's agy customization root holding a
//! `plugin.json` manifest plus a `hooks.json`. Dropping the directory there is
//! enough; no `agy plugin install` / manifest registration is needed. (A
//! `hooks.json` at the CLI root, `~/.gemini/antigravity-cli/`, is parsed and
//! then never executed — do not be tempted by it.)
//!
//! So this module writes one shared, self-contained plugin into the agy root.
//! Two things keep that honest:
//!
//! * **Inert without piki.** Every handler bails with `{}` unless
//!   `PIKI_CLI_AGENT` is set in its env, and piki only sets that on the tabs it
//!   spawns. A plain `agy` run in a normal terminal executes the scripts and
//!   they do nothing.
//! * **No per-spawn state on disk.** The FIFO path rides the environment
//!   (`PIKI_CLI_AGENT_SOCK`), which agy passes down to its hook children, so the
//!   single static `hooks.json` serves every tab.
//!
//! Mapping onto piki's protocol (see [`super::CliAgentEvent`]):
//!
//! | agy hook       | piki event      | resulting status |
//! |----------------|-----------------|------------------|
//! | `PreInvocation`| `prompt_submit` | `Running`        |
//! | `PostToolUse`  | `tool_complete` | `Running`        |
//! | `Stop`         | `stop`          | `Done`           |
//!
//! `PreToolUse` is deliberately NOT registered: agy fires it before *every*
//! tool step regardless of whether the user will actually be asked to approve
//! it, so it carries no permission signal — wiring it up would paint tabs as
//! `WaitingPermission` on every auto-approved tool call. Antigravity tabs
//! therefore never show the permission state that Claude tabs do; everything
//! else is at parity.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use super::install::{jq_available, unique_sock_name, set_executable, CLI_AGENT_TARGET};

const SCRIPT_PAYLOAD: &str = include_str!("scripts/agy-payload.sh");
const SCRIPT_PRE_INVOCATION: &str = include_str!("scripts/agy-on-pre-invocation.sh");
const SCRIPT_POST_TOOL_USE: &str = include_str!("scripts/agy-on-post-tool-use.sh");
const SCRIPT_STOP: &str = include_str!("scripts/agy-on-stop.sh");

/// Plugin directory name under the agy plugins root. Also the hook name inside
/// `hooks.json` (agy merges same-event handlers across plugins by name).
pub const PLUGIN_NAME: &str = "piki-multi-bridge";

/// Env vars to merge into the `agy` child so its hooks report to this tab.
/// No `extra_args`: the plugin is discovered from the agy root, not passed on
/// the command line.
#[derive(Debug, Default, Clone)]
pub struct AntigravityHookSetup {
    pub env: HashMap<String, String>,
    /// Per-spawn FIFO the hook scripts write their payload to. The PTY layer
    /// owns `mkfifo`/cleanup; we only decide the path.
    pub sock_path: Option<PathBuf>,
}

/// The agy plugins root: `~/.gemini/config/plugins`. Plugins dropped here are
/// auto-discovered by every `agy` invocation.
pub fn plugins_root() -> PathBuf {
    crate::xdg::home_dir().join(".gemini/config/plugins")
}

/// Install the bridge plugin and build the env for one `agy` spawn.
///
/// `sock_base` is a piki-owned directory (the FIFO lives under `<sock_base>/
/// sock`); `plugins_root` is the agy customization root the plugin is written
/// to — injectable so tests don't touch the real `~/.gemini`.
///
/// Returns [`io::ErrorKind::NotFound`] when `jq` is missing, exactly like the
/// Claude path: the caller then spawns bare and the heuristic idle watcher
/// stays as the fallback.
pub fn setup_for_antigravity(
    sock_base: &Path,
    plugins_root: &Path,
) -> io::Result<AntigravityHookSetup> {
    if !jq_available() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "`jq` not found on PATH — required for the structured Antigravity integration",
        ));
    }
    materialize(sock_base, plugins_root)
}

/// Write the plugin + decide the FIFO path. Split out so tests can exercise
/// materialization without depending on `jq`.
fn materialize(sock_base: &Path, plugins_root: &Path) -> io::Result<AntigravityHookSetup> {
    let plugin_dir = plugins_root.join(PLUGIN_NAME);
    std::fs::create_dir_all(&plugin_dir)?;

    // Overwritten on every spawn so the scripts stay in sync with the binary.
    let scripts: [(&str, &str, bool); 4] = [
        ("agy-payload.sh", SCRIPT_PAYLOAD, false),
        ("agy-on-pre-invocation.sh", SCRIPT_PRE_INVOCATION, true),
        ("agy-on-post-tool-use.sh", SCRIPT_POST_TOOL_USE, true),
        ("agy-on-stop.sh", SCRIPT_STOP, true),
    ];
    for (name, contents, executable) in scripts {
        let path = plugin_dir.join(name);
        std::fs::write(&path, contents)?;
        if executable {
            set_executable(&path)?;
        }
    }

    std::fs::write(plugin_dir.join("plugin.json"), plugin_json())?;
    std::fs::write(plugin_dir.join("hooks.json"), hooks_json(&plugin_dir))?;

    let sock_dir = sock_base.join("sock");
    std::fs::create_dir_all(&sock_dir)?;
    let sock_path = sock_dir.join(unique_sock_name());

    let mut env = HashMap::new();
    env.insert("PIKI_CLI_AGENT".to_string(), "1".to_string());
    env.insert(
        "PIKI_CLI_AGENT_TARGET".to_string(),
        CLI_AGENT_TARGET.to_string(),
    );
    env.insert(
        "PIKI_CLI_AGENT_V".to_string(),
        super::CLI_AGENT_PROTOCOL_VERSION.to_string(),
    );
    env.insert(
        "PIKI_CLI_AGENT_SOCK".to_string(),
        sock_path.display().to_string(),
    );

    Ok(AntigravityHookSetup {
        env,
        sock_path: Some(sock_path),
    })
}

fn plugin_json() -> String {
    let v = serde_json::json!({
        "name": PLUGIN_NAME,
        "description": "piki-multi agent status bridge (inert unless launched from piki)",
    });
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string())
}

/// A hook command: `sh '<abs script path>'`. Invoked via `sh` (not the
/// executable bit alone) so it survives a `noexec` mount, with the path
/// single-quoted so spaces are safe. agy runs handlers with the working
/// directory set to the plugin dir, but absolute paths keep that irrelevant.
fn hook_command(plugin_dir: &Path, script: &str) -> String {
    let p = plugin_dir.join(script);
    format!("sh '{}'", p.display().to_string().replace('\'', "'\\''"))
}

fn hooks_json(plugin_dir: &Path) -> String {
    let handler = |script: &str| {
        serde_json::json!([{
            "type": "command",
            "command": hook_command(plugin_dir, script),
            "timeout": 10,
        }])
    };
    // Tool-scoped events need the matcher/hooks wrapper; `*` is every tool.
    let handler_matched = |script: &str| {
        serde_json::json!([{
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": hook_command(plugin_dir, script),
                "timeout": 10,
            }],
        }])
    };
    let v = serde_json::json!({
        PLUGIN_NAME: {
            "PreInvocation": handler("agy-on-pre-invocation.sh"),
            "PostToolUse": handler_matched("agy-on-post-tool-use.sh"),
            "Stop": handler("agy-on-stop.sh"),
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn materialized() -> (tempfile::TempDir, tempfile::TempDir, AntigravityHookSetup) {
        let sock_base = tempfile::tempdir().unwrap();
        let plugins = tempfile::tempdir().unwrap();
        let setup = materialize(sock_base.path(), plugins.path()).unwrap();
        (sock_base, plugins, setup)
    }

    #[test]
    fn materialize_writes_a_self_contained_plugin() {
        let (_sock_base, plugins, _setup) = materialized();
        let dir = plugins.path().join(PLUGIN_NAME);
        for name in [
            "plugin.json",
            "hooks.json",
            "agy-payload.sh",
            "agy-on-pre-invocation.sh",
            "agy-on-post-tool-use.sh",
            "agy-on-stop.sh",
        ] {
            assert!(dir.join(name).exists(), "missing {name}");
        }
    }

    #[test]
    fn hooks_json_registers_the_three_lifecycle_events() {
        let (_sock_base, plugins, _setup) = materialized();
        let dir = plugins.path().join(PLUGIN_NAME);
        let raw = std::fs::read_to_string(dir.join("hooks.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        let spec = v.get(PLUGIN_NAME).and_then(|s| s.as_object()).expect("spec");

        for key in ["PreInvocation", "PostToolUse", "Stop"] {
            assert!(spec.contains_key(key), "missing hook {key}");
        }
        // PreToolUse would fire on every tool step, permission or not — see the
        // module docs. Registering it would mis-paint tabs as WaitingPermission.
        assert!(!spec.contains_key("PreToolUse"));

        // Flat events take a bare handler list; tool events need the wrapper.
        let stop = spec["Stop"][0]["command"].as_str().unwrap();
        assert!(stop.starts_with("sh '"));
        assert!(stop.contains("agy-on-stop.sh"));
        assert_eq!(spec["PostToolUse"][0]["matcher"], "*");
        let post = spec["PostToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(post.contains("agy-on-post-tool-use.sh"));
    }

    #[test]
    fn env_advertises_the_fifo_but_does_not_create_it() {
        let (sock_base, _plugins, setup) = materialized();
        assert_eq!(setup.env.get("PIKI_CLI_AGENT").unwrap(), "1");
        assert_eq!(
            setup.env.get("PIKI_CLI_AGENT_TARGET").unwrap(),
            CLI_AGENT_TARGET
        );
        assert_eq!(
            setup.env.get("PIKI_CLI_AGENT_V").unwrap(),
            &super::super::CLI_AGENT_PROTOCOL_VERSION.to_string()
        );

        let sock = setup.sock_path.clone().expect("sock_path is Some");
        assert_eq!(
            setup.env.get("PIKI_CLI_AGENT_SOCK").unwrap(),
            &sock.display().to_string()
        );
        assert!(sock.starts_with(sock_base.path().join("sock")));
        assert!(
            !sock.exists(),
            "install must not create the FIFO; the PTY layer owns it"
        );
    }

    #[test]
    fn sock_path_is_unique_across_spawns() {
        let sock_base = tempfile::tempdir().unwrap();
        let plugins = tempfile::tempdir().unwrap();
        let a = materialize(sock_base.path(), plugins.path())
            .unwrap()
            .sock_path
            .unwrap();
        let b = materialize(sock_base.path(), plugins.path())
            .unwrap()
            .sock_path
            .unwrap();
        assert_ne!(a, b, "each spawn must get a distinct FIFO path");
    }

    #[test]
    fn materialize_is_idempotent_and_repairs_the_scripts() {
        let sock_base = tempfile::tempdir().unwrap();
        let plugins = tempfile::tempdir().unwrap();
        materialize(sock_base.path(), plugins.path()).unwrap();
        let stop = plugins.path().join(PLUGIN_NAME).join("agy-on-stop.sh");
        std::fs::write(&stop, b"corrupted").unwrap();
        materialize(sock_base.path(), plugins.path()).unwrap();
        let contents = std::fs::read_to_string(&stop).unwrap();
        assert!(contents.contains("piki cli-agent `stop` event"));
    }

    /// Every handler must print a JSON object on stdout even when piki isn't
    /// driving it — agy treats a handler that prints nothing as failed, and the
    /// plugin lives in the user's global agy config, so a bare `agy` run in a
    /// normal terminal executes these scripts too.
    #[test]
    fn scripts_are_inert_without_piki_env() {
        let (_sock_base, plugins, _setup) = materialized();
        let dir = plugins.path().join(PLUGIN_NAME);
        for script in [
            "agy-on-pre-invocation.sh",
            "agy-on-post-tool-use.sh",
            "agy-on-stop.sh",
        ] {
            let out = std::process::Command::new("sh")
                .arg(dir.join(script))
                .env_remove("PIKI_CLI_AGENT")
                .output()
                .expect("hook script runs");
            assert!(out.status.success(), "{script} exited non-zero");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let v: serde_json::Value =
                serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
                    panic!("{script} must print a JSON object, got {stdout:?}: {e}")
                });
            assert!(v.is_object(), "{script} printed {v}");
        }
    }
}
