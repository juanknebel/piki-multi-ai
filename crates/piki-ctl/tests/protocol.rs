//! `piki-ctl`'s wire format is deliberately duplicated (not imported — see
//! `src/main.rs`'s module doc) from `piki_core::cli_agent::open_action`, so
//! the standalone binary doesn't drag in piki-core's dependency graph. This
//! test is the drift guard: it runs the real `piki-ctl` binary and feeds its
//! real stdout to piki-core's real parser, the same one piki's desktop/TUI
//! frontends use.

use std::process::Command;

use piki_core::cli_agent::{CliAgentEvent, parse_cli_agent_payload};

#[test]
fn piki_ctl_open_file_output_parses_as_a_real_cli_agent_open_event() {
    let exe = env!("CARGO_BIN_EXE_piki-ctl");
    let out = Command::new(exe)
        .args(["open", "file", "/abs/ble.md"])
        .output()
        .expect("failed to run piki-ctl");
    assert!(out.status.success(), "piki-ctl exited non-zero");

    let osc = String::from_utf8(out.stdout).expect("stdout wasn't utf8");
    let prefix = "\x1b]777;notify;piki://cli-agent;";
    let json = osc
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix('\x07'))
        .expect("piki-ctl output wasn't the expected OSC 777 framing");

    let ev = parse_cli_agent_payload(json).expect("piki-core's parser rejected piki-ctl's output");
    assert_eq!(
        ev,
        CliAgentEvent::Open {
            session_id: String::new(),
            target_kind: "file".to_string(),
            path: "/abs/ble.md".to_string(),
        }
    );
}
