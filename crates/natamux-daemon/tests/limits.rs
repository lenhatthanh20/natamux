//! The fixtures keep the last Node limits as a regression reference after cutover.

#![allow(clippy::unwrap_used, clippy::panic)]

use natamux_daemon::terminals::limits::*;
use natamux_daemon::terminals::open_shells::{
    EXIT_CONFIRM_MS, RESTORE_POLL_MS, RESTORE_SPINNER_MS,
};

const LIMITS_TS: &str = include_str!("fixtures/terminal-limits.ts");
const OPEN_SHELLS_TS: &str = include_str!("fixtures/open-shells.ts");

/// The right side of `export const NAME = <value>`: `64 * 1024` and `30_000` work.
fn number(source: &str, name: &str) -> u64 {
    let marker = format!("export const {name} = ");
    let start = source.find(&marker).unwrap_or_else(|| panic!("no {name}")) + marker.len();
    let line = source[start..].lines().next().unwrap();
    line.replace('_', "")
        .split('*')
        .map(|factor| factor.trim().parse::<u64>().unwrap())
        .product()
}

#[test]
fn terminal_limits_match_the_node_daemon() {
    // MAX_SESSIONS and OUTPUT_RING_BYTES moved on from the Node values in T-0210.
    let pairs: [(&str, u64); 19] = [
        ("TICKET_TTL_MS", TICKET_TTL_MS),
        ("OUTPUT_COALESCE_MS", OUTPUT_COALESCE_MS),
        ("OUTPUT_IDLE_BYTES", OUTPUT_IDLE_BYTES as u64),
        ("OUTPUT_FRAME_BYTES", OUTPUT_FRAME_BYTES as u64),
        ("SNAPSHOT_SYNC_WAIT_MS", SNAPSHOT_SYNC_WAIT_MS),
        ("CLIENT_HIGH_WATER", CLIENT_HIGH_WATER),
        ("HIDDEN_BUDGET_BYTES", HIDDEN_BUDGET_BYTES),
        ("HIDDEN_BUDGET_WINDOW_MS", HIDDEN_BUDGET_WINDOW_MS),
        ("RESYNC_QUIET_WINDOW_MS", RESYNC_QUIET_WINDOW_MS),
        ("RESYNC_MAX_WAIT_MS", RESYNC_MAX_WAIT_MS),
        ("ACK_STALL_MS", ACK_STALL_MS),
        ("KILL_GRACE_MS", KILL_GRACE_MS),
        ("EXIT_WAIT_MS", EXIT_WAIT_MS),
        ("INPUT_GAP_MS", INPUT_GAP_MS),
        ("ENTER_AFTER_PASTE_MS", ENTER_AFTER_PASTE_MS),
        ("ENDED_IDLE_MS", ENDED_IDLE_MS),
        ("WORKSPACE_QUIET_MS", WORKSPACE_QUIET_MS),
        ("RESTORE_SPINNER_MS", RESTORE_SPINNER_MS),
        ("RESTORE_POLL_MS", RESTORE_POLL_MS),
    ];
    for (name, rust) in pairs {
        let source = if name.starts_with("RESTORE") {
            OPEN_SHELLS_TS
        } else {
            LIMITS_TS
        };
        assert_eq!(number(source, name), rust, "{name}");
    }
    assert_eq!(number(OPEN_SHELLS_TS, "EXIT_CONFIRM_MS"), EXIT_CONFIRM_MS);
    assert!(LIMITS_TS.contains(&format!("export const MAIN_TERMINAL = '{MAIN_TERMINAL}'")));
}
