//! End-to-end check of the Antigravity (`agy`) hook bridge.
//!
//! Ignored by default: it drives the real `agy` binary, which needs the user to
//! be logged in and burns model quota. Run it by hand after touching the bridge:
//!
//! ```sh
//! cargo test -p piki-core --test cli_agent_antigravity -- --ignored --nocapture
//! ```
//!
//! It exercises the exact path `spawn_tab` takes: materialize the bridge plugin
//! into agy's real customization root, mkfifo the per-tab socket, spawn `agy`
//! with the bridge env, and assert the lifecycle events come back down the FIFO.
//! It writes to `~/.gemini/config/plugins/piki-multi-bridge` — the same shared,
//! inert-without-piki plugin the app installs at runtime — and leaves it there.

#![cfg(unix)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use piki_core::cli_agent::{CliAgentEvent, install_antigravity as agy};
use piki_core::pty::ShellSession;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives the real `agy` binary: needs login + model quota"]
async fn agy_lifecycle_events_reach_the_fifo() {
    if which("agy").is_none() {
        eprintln!("skipping: `agy` not on PATH");
        return;
    }

    let sock_base = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();

    let setup = agy::setup_for_antigravity(sock_base.path(), &agy::plugins_root())
        .expect("bridge install (needs jq)");
    let sock_path = setup.sock_path.clone().expect("sock path");

    // Stand up the reader exactly like the PTY layer does, then let the hooks
    // write into it.
    let shell = Arc::new(Mutex::new(ShellSession::default()));
    let _reader = piki_core::cli_agent::sock::spawn_reader(sock_path, Arc::clone(&shell), None)
        .expect("fifo reader");

    let mut cmd = std::process::Command::new("agy");
    cmd.current_dir(workspace.path())
        .args(["--print", "say hi in one word"]);
    for (k, v) in &setup.env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("agy runs");
    assert!(
        out.status.success(),
        "agy failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The Stop hook sleeps briefly to let the transcript flush, so the last
    // payload can land after the process exits.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !has_stop(&shell) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let events = drain(&shell);

    let cli_agent: Vec<&CliAgentEvent> = events
        .iter()
        .filter_map(|e| match e {
            piki_core::shell_integration::ShellEvent::CliAgent(ev) => Some(ev),
            _ => None,
        })
        .collect();
    assert!(
        !cli_agent.is_empty(),
        "no cli-agent events came back from agy"
    );

    // PreInvocation must open the turn (Running) and Stop must close it (Done).
    assert!(
        cli_agent
            .iter()
            .any(|e| matches!(e, CliAgentEvent::UserPromptSubmit { .. })),
        "missing prompt_submit, got {cli_agent:?}"
    );
    let stop = cli_agent
        .iter()
        .find(|e| matches!(e, CliAgentEvent::Stop { .. }))
        .unwrap_or_else(|| panic!("missing stop, got {cli_agent:?}"));
    let CliAgentEvent::Stop {
        session_id,
        response,
        transcript_path,
        ..
    } = stop
    else {
        unreachable!()
    };
    assert!(!session_id.is_empty(), "stop carries agy's conversationId");
    assert!(
        transcript_path.is_some(),
        "stop carries agy's transcriptPath"
    );
    assert!(
        response.as_deref().is_some_and(|r| !r.is_empty()),
        "stop carries the agent's reply preview, got {response:?}"
    );
}

fn has_stop(shell: &Arc<Mutex<ShellSession>>) -> bool {
    shell.lock().pending_events.iter().any(|e| {
        matches!(
            e,
            piki_core::shell_integration::ShellEvent::CliAgent(CliAgentEvent::Stop { .. })
        )
    })
}

fn drain(shell: &Arc<Mutex<ShellSession>>) -> Vec<piki_core::shell_integration::ShellEvent> {
    shell.lock().pending_events.drain(..).collect()
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(bin))
            .find(|p| p.is_file())
    })
}
