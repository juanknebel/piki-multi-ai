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
//! | agy hook        | piki event           | resulting status    |
//! |-----------------|------------------------|---------------------|
//! | `PreInvocation` | `prompt_submit`      | `Running`           |
//! | `PreToolUse`    | `permission_request` | `WaitingPermission` |
//! |                 | or `tool_started`    | or `Running`        |
//! | `PostToolUse`   | `tool_complete`      | `Running`           |
//! | `Stop`          | `stop`               | `Done`              |
//!
//! `PreToolUse` needs a prediction step: agy fires it before *every* tool step
//! regardless of whether the user will actually be asked to approve it, and
//! agy 1.2.5 exposes no permission hook event (its embedded `hooks.json` docs
//! list exactly `PreToolUse`, `PostToolUse`, `PreInvocation`,
//! `PostInvocation` and `Stop`). So the handler predicts the permission gate
//! from the on-disk signals agy documents in `settings.json` — a `run_command`
//! the `permissions.allow` list doesn't cover, outside `trustedWorkspaces`,
//! and under a default `toolPermission` mode becomes `permission_request`
//! (amber `WaitingPermission`, like Claude tabs); everything else becomes
//! `tool_started` (plain `Running`). Anything unknown fails safe to
//! `tool_started` — never amber without evidence — and a mispredicted amber
//! is corrected by the follow-up `tool_complete`, which always resets to
//! `Running`. The hook is a pure observer: it always prints `{}` and never
//! overrides agy's own permission policy.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use super::CLI_AGENT_TARGET;
use super::install::{jq_available, set_executable, unique_sock_name};

const SCRIPT_PAYLOAD: &str = include_str!("scripts/agy-payload.sh");
const SCRIPT_PRE_INVOCATION: &str = include_str!("scripts/agy-on-pre-invocation.sh");
const SCRIPT_PRE_TOOL_USE: &str = include_str!("scripts/agy-on-pre-tool-use.sh");
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
    setup_for_antigravity_with_sock(sock_base, plugins_root, None)
}

/// [`setup_for_antigravity`] with a caller-chosen FIFO file name (the
/// session daemon derives it from the session id). `None` picks a
/// process-unique name.
pub fn setup_for_antigravity_with_sock(
    sock_base: &Path,
    plugins_root: &Path,
    sock_name: Option<&str>,
) -> io::Result<AntigravityHookSetup> {
    if !jq_available() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "`jq` not found on PATH — required for the structured Antigravity integration",
        ));
    }
    materialize(sock_base, plugins_root, sock_name)
}

/// Write the plugin + decide the FIFO path. Split out so tests can exercise
/// materialization without depending on `jq`.
fn materialize(
    sock_base: &Path,
    plugins_root: &Path,
    sock_name: Option<&str>,
) -> io::Result<AntigravityHookSetup> {
    let plugin_dir = plugins_root.join(PLUGIN_NAME);
    std::fs::create_dir_all(&plugin_dir)?;

    // Overwritten on every spawn so the scripts stay in sync with the binary.
    let scripts: [(&str, &str, bool); 5] = [
        ("agy-payload.sh", SCRIPT_PAYLOAD, false),
        ("agy-on-pre-invocation.sh", SCRIPT_PRE_INVOCATION, true),
        ("agy-on-pre-tool-use.sh", SCRIPT_PRE_TOOL_USE, true),
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
    let sock_path = sock_dir.join(sock_name.map_or_else(unique_sock_name, str::to_string));

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
            "PreToolUse": handler_matched("agy-on-pre-tool-use.sh"),
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
        let setup = materialize(sock_base.path(), plugins.path(), None).unwrap();
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
            "agy-on-pre-tool-use.sh",
            "agy-on-post-tool-use.sh",
            "agy-on-stop.sh",
        ] {
            assert!(dir.join(name).exists(), "missing {name}");
        }
    }

    #[test]
    fn hooks_json_registers_the_four_lifecycle_events() {
        let (_sock_base, plugins, _setup) = materialized();
        let dir = plugins.path().join(PLUGIN_NAME);
        let raw = std::fs::read_to_string(dir.join("hooks.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        let spec = v
            .get(PLUGIN_NAME)
            .and_then(|s| s.as_object())
            .expect("spec");

        for key in ["PreInvocation", "PreToolUse", "PostToolUse", "Stop"] {
            assert!(spec.contains_key(key), "missing hook {key}");
        }

        // Flat events take a bare handler list; tool events need the wrapper.
        let stop = spec["Stop"][0]["command"].as_str().unwrap();
        assert!(stop.starts_with("sh '"));
        assert!(stop.contains("agy-on-stop.sh"));
        // PreToolUse fires on every tool step, so it is registered as an
        // observer whose script predicts the permission gate (see module
        // docs) — the matcher wrapper is what makes it a tool event.
        assert_eq!(spec["PreToolUse"][0]["matcher"], "*");
        let pre = spec["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(pre.contains("agy-on-pre-tool-use.sh"));
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
        let a = materialize(sock_base.path(), plugins.path(), None)
            .unwrap()
            .sock_path
            .unwrap();
        let b = materialize(sock_base.path(), plugins.path(), None)
            .unwrap()
            .sock_path
            .unwrap();
        assert_ne!(a, b, "each spawn must get a distinct FIFO path");
    }

    #[test]
    fn materialize_is_idempotent_and_repairs_the_scripts() {
        let sock_base = tempfile::tempdir().unwrap();
        let plugins = tempfile::tempdir().unwrap();
        materialize(sock_base.path(), plugins.path(), None).unwrap();
        let stop = plugins.path().join(PLUGIN_NAME).join("agy-on-stop.sh");
        std::fs::write(&stop, b"corrupted").unwrap();
        materialize(sock_base.path(), plugins.path(), None).unwrap();
        let contents = std::fs::read_to_string(&stop).unwrap();
        assert!(contents.contains("piki cli-agent `stop` event"));
    }

    /// The PreToolUse prediction, driven end to end: fake `$HOME` carries a
    /// `settings.json`, a real FIFO captures the payload, crafted stdin feeds
    /// the tool call. Only an uncovered, untrusted `run_command` under a
    /// default mode predicts a prompt — everything else fails safe to
    /// `tool_started`.
    #[cfg(unix)]
    #[test]
    fn pre_tool_use_predicts_permission_from_settings() {
        let (_sock_base, plugins, _setup) = materialized();
        let dir = plugins.path().join(PLUGIN_NAME);

        let home = tempfile::tempdir().unwrap();
        let settings_dir = home.path().join(".gemini/antigravity-cli");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"permissions": {"allow": ["command(git status)", "command(cargo test)"]},
                "trustedWorkspaces": ["/trusted/ws"]}"#,
        )
        .unwrap();

        let fifo_dir = tempfile::tempdir().unwrap();
        let fifo = fifo_dir.path().join("fifo");
        let mkfifo = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo runs");
        assert!(mkfifo.success());

        let run = |stdin: &str| -> serde_json::Value {
            let script = dir.join("agy-on-pre-tool-use.sh");
            let fifo_clone = fifo.clone();
            let reader = std::thread::spawn(move || {
                std::fs::read_to_string(&fifo_clone).expect("fifo read")
            });
            let out = std::process::Command::new("sh")
                .arg(&script)
                .env("HOME", home.path())
                .env("PIKI_CLI_AGENT", "1")
                .env("PIKI_CLI_AGENT_V", "1")
                .env("PIKI_CLI_AGENT_SOCK", &fifo)
                .env("PIKI_CLI_AGENT_TARGET", super::super::CLI_AGENT_TARGET)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("hook script runs");
            let mut child = out;
            use std::io::Write as _;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            drop(child.stdin.take());
            let finished = child.wait_with_output().expect("hook exits");
            assert!(finished.status.success());
            // Observer only: stdout stays the empty approval object.
            assert_eq!(String::from_utf8_lossy(&finished.stdout).trim(), "{}");
            let line = reader.join().expect("reader joins");
            serde_json::from_str(line.trim()).expect("payload is JSON")
        };

        let tool_stdin = |ws: &str, name: &str, args: &str| -> String {
            format!(
                r#"{{"conversationId":"c","workspacePaths":["{ws}"],"toolCall":{{"name":"{name}","args":{args}}}}}"#
            )
        };

        // Covered by the allow list (word-prefix): no prompt coming.
        let v = run(&tool_stdin(
            "/other/ws",
            "run_command",
            r#"{"CommandLine":"cargo test --lib foo"}"#,
        ));
        assert_eq!(v["event"], "tool_started");

        // Same prefix as a word, but not the same words: prompt coming.
        let v = run(&tool_stdin(
            "/other/ws",
            "run_command",
            r#"{"CommandLine":"cargo testing foo"}"#,
        ));
        assert_eq!(v["event"], "permission_request");
        assert!(
            v["summary"]
                .as_str()
                .unwrap()
                .starts_with("Wants to run run_command")
        );

        // Uncovered command, but the checkout is trusted: no prompt.
        let v = run(&tool_stdin(
            "/trusted/ws/sub",
            "run_command",
            r#"{"CommandLine":"rm -rf /tmp/x"}"#,
        ));
        assert_eq!(v["event"], "tool_started");

        // A tool piki can't build a permission string for: fail safe.
        let v = run(&tool_stdin("/other/ws", "view_file", r#"{"Path":"/x"}"#));
        assert_eq!(v["event"], "tool_started");
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
            "agy-on-pre-tool-use.sh",
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
            let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
                panic!("{script} must print a JSON object, got {stdout:?}: {e}")
            });
            assert!(v.is_object(), "{script} printed {v}");
        }
    }
}
