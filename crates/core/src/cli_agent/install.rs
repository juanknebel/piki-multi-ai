//! Materialize the embedded Claude Code hook scripts to disk and produce the
//! env vars + CLI args that make `claude` load them — *without* touching the
//! user's own `~/.claude/settings.json`.
//!
//! We generate a standalone settings file and pass it via
//! `claude --settings <file>`; its `hooks` block points at the materialized
//! scripts. The scripts are no-ops unless `PIKI_CLI_AGENT` is set in their
//! env, so the file is inert if it ever leaks outside a piki-spawned tab.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SCRIPT_BUILD_PAYLOAD: &str = include_str!("scripts/build-payload.sh");
const SCRIPT_SESSION_START: &str = include_str!("scripts/on-session-start.sh");
const SCRIPT_PROMPT_SUBMIT: &str = include_str!("scripts/on-prompt-submit.sh");
const SCRIPT_POST_TOOL_USE: &str = include_str!("scripts/on-post-tool-use.sh");
const SCRIPT_PERMISSION: &str = include_str!("scripts/on-permission-request.sh");
const SCRIPT_NOTIFICATION: &str = include_str!("scripts/on-notification.sh");
const SCRIPT_STOP: &str = include_str!("scripts/on-stop.sh");

/// In-band OSC target token. The parser only accepts OSC 777 sequences whose
/// target equals this exact string (collision guard vs. Warp's `warp://`,
/// urxvt notify, VTE, …). Keep in sync with the `parser` 777 arm.
pub const CLI_AGENT_TARGET: &str = "piki://cli-agent";

/// Env vars + extra CLI args to merge into the `claude` child so it loads
/// piki's hooks and the scripts know they're allowed to emit.
#[derive(Debug, Default, Clone)]
pub struct ClaudeHookSetup {
    pub env: HashMap<String, String>,
    /// Args to **prepend** to the command's normal args (before the prompt).
    pub extra_args: Vec<String>,
    /// Per-spawn FIFO path advertised to the `claude` child via
    /// `PIKI_CLI_AGENT_SOCK`. The hook scripts prefer writing the structured
    /// payload here (out-of-band) and fall back to the in-band OSC 777
    /// `/dev/tty` write only when this is absent. The file itself is NOT
    /// created here — the PTY layer owns `mkfifo`/cleanup; install only
    /// decides the path. `None` only if the path could not be derived.
    pub sock_path: Option<PathBuf>,
}

/// Materialize the hook scripts under `base_dir` (idempotent — overwrites to
/// stay in sync with the binary), generate the settings file, and return the
/// env/args needed so `claude` picks them up.
///
/// `base_dir` should be a stable per-user location
/// (e.g. `<data_dir>/claude-hooks`).
///
/// Returns [`io::ErrorKind::NotFound`] when `jq` is not on PATH — the hook
/// scripts require it to build the JSON payload. Callers treat that as
/// "spawn bare" (no hooks); the heuristic idle watcher then stays as the
/// graceful fallback (see the idle-loop guard in TUI/desktop).
pub fn setup_for_claude(base_dir: &Path) -> io::Result<ClaudeHookSetup> {
    if !jq_available() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "`jq` not found on PATH — required for the structured Claude integration",
        ));
    }
    materialize(base_dir)
}

/// `true` when a working `jq` is reachable. Resolves via the user's *login*
/// PATH (same path resolution used to spawn `claude`), not the bare process
/// PATH — a GUI/.desktop launch inherits a minimal env where
/// `Command::new("jq")`'s built-in lookup spuriously fails even though jq is
/// installed.
pub fn jq_available() -> bool {
    let jq = crate::shell_env::resolve_command("jq");
    crate::shell_env::sync_command(&jq)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write the scripts + settings and build the env/args. Split out so tests
/// can exercise materialization without depending on `jq` being installed.
fn materialize(base_dir: &Path) -> io::Result<ClaudeHookSetup> {
    let scripts_dir = base_dir.join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    let scripts: [(&str, &str, bool); 7] = [
        ("build-payload.sh", SCRIPT_BUILD_PAYLOAD, false),
        ("on-session-start.sh", SCRIPT_SESSION_START, true),
        ("on-prompt-submit.sh", SCRIPT_PROMPT_SUBMIT, true),
        ("on-post-tool-use.sh", SCRIPT_POST_TOOL_USE, true),
        ("on-permission-request.sh", SCRIPT_PERMISSION, true),
        ("on-notification.sh", SCRIPT_NOTIFICATION, true),
        ("on-stop.sh", SCRIPT_STOP, true),
    ];
    for (name, contents, executable) in scripts {
        let path = scripts_dir.join(name);
        std::fs::write(&path, contents)?;
        if executable {
            set_executable(&path)?;
        }
    }

    let settings_path = base_dir.join("settings.json");
    std::fs::write(&settings_path, settings_json(&scripts_dir))?;

    // Compute a unique-per-spawn FIFO path under `<base_dir>/sock`. The PTY
    // layer mkfifo's + cleans it up; we only decide the path and advertise it
    // via env so it propagates to the `claude` child and onward to its hooks.
    let sock_dir = base_dir.join("sock");
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
    // Advertised so the shell-integration bridge can wrap a manually-typed
    // `claude` with `--settings <this file>` inside shell tabs (where piki
    // doesn't control the command line).
    env.insert(
        "PIKI_CLAUDE_HOOK_SETTINGS".to_string(),
        settings_path.display().to_string(),
    );

    Ok(ClaudeHookSetup {
        env,
        extra_args: vec![
            "--settings".to_string(),
            settings_path.display().to_string(),
        ],
        sock_path: Some(sock_path),
    })
}

/// A process-unique FIFO file name: pid + a process-global monotonic counter +
/// wall-clock nanos. Collision-free across concurrent spawns in the same
/// process (counter) and across separate piki processes (pid + nanos).
pub(super) fn unique_sock_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("cli-agent-{pid}-{n}-{nanos}.sock")
}

#[cfg(unix)]
pub(super) fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
pub(super) fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// A hook command string: `sh '<abs script path>'`. We invoke via `sh` (not
/// the executable bit alone) so it works even on filesystems mounted `noexec`,
/// and single-quote the path so spaces / odd chars in `--data-dir` are safe.
fn hook_command(scripts_dir: &Path, script: &str) -> String {
    let p = scripts_dir.join(script);
    format!("sh '{}'", p.display().to_string().replace('\'', "'\\''"))
}

fn settings_json(scripts_dir: &Path) -> String {
    let entry = |script: &str| {
        serde_json::json!([{
            "hooks": [{ "type": "command", "command": hook_command(scripts_dir, script) }]
        }])
    };
    let entry_matched = |matcher: &str, script: &str| {
        serde_json::json!([{
            "matcher": matcher,
            "hooks": [{ "type": "command", "command": hook_command(scripts_dir, script) }]
        }])
    };
    let v = serde_json::json!({
        "hooks": {
            "SessionStart": entry_matched("startup|resume", "on-session-start.sh"),
            "UserPromptSubmit": entry("on-prompt-submit.sh"),
            "PostToolUse": entry("on-post-tool-use.sh"),
            "PermissionRequest": entry("on-permission-request.sh"),
            "Notification": entry_matched("idle_prompt", "on-notification.sh"),
            "Stop": entry("on-stop.sh"),
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_writes_scripts_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        // Exercise materialization directly so the test doesn't depend on
        // `jq` being installed on the runner.
        let setup = materialize(dir.path()).unwrap();

        let scripts = dir.path().join("scripts");
        for name in [
            "build-payload.sh",
            "on-session-start.sh",
            "on-prompt-submit.sh",
            "on-post-tool-use.sh",
            "on-permission-request.sh",
            "on-notification.sh",
            "on-stop.sh",
        ] {
            assert!(scripts.join(name).exists(), "missing {name}");
        }

        let settings_path = dir.path().join("settings.json");
        assert!(settings_path.exists());
        assert_eq!(setup.extra_args[0], "--settings");
        assert_eq!(setup.extra_args[1], settings_path.display().to_string());
        assert_eq!(
            setup.env.get("PIKI_CLAUDE_HOOK_SETTINGS").unwrap(),
            &settings_path.display().to_string()
        );
        assert_eq!(setup.env.get("PIKI_CLI_AGENT").unwrap(), "1");
        assert_eq!(
            setup.env.get("PIKI_CLI_AGENT_TARGET").unwrap(),
            CLI_AGENT_TARGET
        );
        assert_eq!(
            setup.env.get("PIKI_CLI_AGENT_V").unwrap(),
            &super::super::CLI_AGENT_PROTOCOL_VERSION.to_string()
        );

        // The out-of-band FIFO transport: a path is decided and advertised via
        // env, but the file itself is NOT created here (PTY layer owns that).
        let sock = setup.sock_path.clone().expect("sock_path is Some");
        assert_eq!(
            setup.env.get("PIKI_CLI_AGENT_SOCK").unwrap(),
            &sock.display().to_string()
        );
        assert!(sock.starts_with(dir.path().join("sock")));
        assert!(
            !sock.exists(),
            "install must not create the FIFO; PTY layer owns it"
        );
        assert!(
            dir.path().join("sock").is_dir(),
            "the sock dir itself is created"
        );
    }

    #[test]
    fn sock_path_is_unique_across_materialize_calls() {
        let dir = tempfile::tempdir().unwrap();
        let a = materialize(dir.path()).unwrap().sock_path.unwrap();
        let b = materialize(dir.path()).unwrap().sock_path.unwrap();
        assert_ne!(a, b, "each spawn must get a distinct FIFO path");
    }

    #[test]
    fn settings_json_is_valid_and_registers_six_hooks() {
        let dir = tempfile::tempdir().unwrap();
        materialize(dir.path()).unwrap();
        let raw = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        let hooks = v.get("hooks").and_then(|h| h.as_object()).expect("hooks");
        for key in [
            "SessionStart",
            "UserPromptSubmit",
            "PostToolUse",
            "PermissionRequest",
            "Notification",
            "Stop",
        ] {
            assert!(hooks.contains_key(key), "missing hook {key}");
        }
        // Command must reference the materialized script via `sh '...'`.
        let cmd = hooks["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(cmd.starts_with("sh '"));
        assert!(cmd.contains("on-stop.sh"));
    }

    #[test]
    fn materialize_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        materialize(dir.path()).unwrap();
        let stop = dir.path().join("scripts/on-stop.sh");
        std::fs::write(&stop, b"corrupted").unwrap();
        materialize(dir.path()).unwrap();
        let contents = std::fs::read_to_string(&stop).unwrap();
        assert!(contents.contains("piki cli-agent `stop` event"));
    }

    #[test]
    fn setup_for_claude_gate_matches_jq_presence() {
        // Deterministic on any machine: the public entry point must succeed
        // iff `jq` is reachable, and error with NotFound otherwise.
        let dir = tempfile::tempdir().unwrap();
        let r = setup_for_claude(dir.path());
        if jq_available() {
            assert!(r.is_ok());
        } else {
            let e = r.expect_err("must error without jq");
            assert_eq!(e.kind(), io::ErrorKind::NotFound);
        }
    }

    #[test]
    fn hook_command_single_quotes_path_with_spaces() {
        let cmd = hook_command(Path::new("/tmp/da ta/scripts"), "on-stop.sh");
        assert_eq!(cmd, "sh '/tmp/da ta/scripts/on-stop.sh'");
    }
}
