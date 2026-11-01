//! `piki-ctl` — the one command any coding agent needs to ask piki's UI to
//! *do* something, without piki knowing or caring which agent it is.
//!
//! Every agent that can run a shell command (Claude Code, Codex, muse-cli,
//! a bare shell — all of them) can run this. There is nothing to install
//! into the agent itself: document the command in the repo's `AGENTS.md`
//! and the agent picks it up like any other CLI tool.
//!
//! How it works: piki's terminal engine already sniffs OSC 777
//! `piki://cli-agent` sequences out of *every* tab's PTY byte stream (that's
//! how Claude Code's own lifecycle hooks report status — see
//! `piki-core::cli_agent`). `piki-ctl` rides the exact same wire format, but
//! for an imperative one-shot action instead of a passive lifecycle event:
//! it just writes the escape sequence to its own stdout, which *is* the
//! PTY. No FIFO, no per-agent hook install, no API calls — the whole
//! integration is "print this one line".
//!
//! Usage:
//! ```text
//! piki-ctl open file <path>     # open <path> in piki's file preview
//! ```
//!
//! `<path>` is resolved against the current working directory, so it works
//! the same whether the agent is sitting at the repo root or three
//! directories deep.
//!
//! The wire format (`CLI_AGENT_TARGET`, the `open` event shape) is
//! intentionally duplicated here rather than imported from `piki-core` —
//! this binary ships standalone to third parties and must not drag in
//! piki-core's much heavier dependency graph (tokio, tauri, ...). Drift
//! between the two copies is caught by `tests/protocol.rs`, which round-trips
//! this binary's real output through piki-core's actual parser.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

/// Mirrors `piki_core::cli_agent::CLI_AGENT_TARGET`. Keep in sync — see the
/// module doc above for why this isn't a shared dependency instead.
const CLI_AGENT_TARGET: &str = "piki://cli-agent";

/// Mirrors `piki_core::cli_agent::open_action::OPEN_EVENT`.
const OPEN_EVENT: &str = "open";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(osc) => {
            print!("{osc}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("piki-ctl: {msg}");
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: piki-ctl open file <path>";

fn run(args: &[String]) -> Result<String, String> {
    match args {
        [verb, kind, target] if verb == "open" => build_open(kind, target),
        _ => Err("expected: open <kind> <target>".to_string()),
    }
}

fn build_open(kind: &str, target: &str) -> Result<String, String> {
    match kind {
        "file" => {
            let path = resolve_path(target)?;
            Ok(open_osc("file", &path))
        }
        other => Err(format!("unknown target kind '{other}' (expected: file)")),
    }
}

/// Resolve `target` to an absolute path against the current working
/// directory — the agent's cwd at the moment it ran `piki-ctl`, which is
/// exactly "from where I'm located" from the agent's point of view.
fn resolve_path(target: &str) -> Result<String, String> {
    let p = PathBuf::from(target);
    let abs = if p.is_absolute() {
        p
    } else {
        env::current_dir()
            .map_err(|e| format!("failed to resolve current directory: {e}"))?
            .join(p)
    };
    Ok(abs.to_string_lossy().into_owned())
}

/// Build the full in-band OSC 777 escape sequence for an `open` action. See
/// `piki_core::cli_agent::open_action::build_open_osc` — the real, imported
/// copy of this logic that the parser side uses; this one must match it.
fn open_osc(target_kind: &str, path: &str) -> String {
    let body = serde_json::json!({
        "v": 1,
        "event": OPEN_EVENT,
        "target_kind": target_kind,
        "path": path,
    });
    format!("\x1b]777;notify;{CLI_AGENT_TARGET};{body}\x07")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_file_resolves_relative_paths() {
        let cwd = env::current_dir().unwrap();
        let osc = run(&["open".to_string(), "file".to_string(), "ble.md".to_string()]).unwrap();
        assert!(osc.contains(&cwd.join("ble.md").to_string_lossy().into_owned()));
    }

    #[test]
    fn open_file_keeps_absolute_paths_as_is() {
        let osc = run(&[
            "open".to_string(),
            "file".to_string(),
            "/abs/ble.md".to_string(),
        ])
        .unwrap();
        assert!(osc.contains("/abs/ble.md"));
    }

    #[test]
    fn rejects_unknown_target_kind() {
        assert!(run(&["open".to_string(), "url".to_string(), "x".to_string()]).is_err());
    }

    #[test]
    fn rejects_unknown_verb() {
        assert!(run(&["close".to_string(), "file".to_string(), "x".to_string()]).is_err());
    }
}
