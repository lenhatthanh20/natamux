//! The terminal limits copied from `packages/core` for the Rust daemon (phase 4). Fails when the
//! TypeScript changes and this crate does not.

#![allow(clippy::unwrap_used, clippy::panic)]

use natamux_proto::tasks::ports::{PORT_BASE, PORTS_PER_TASK};
use natamux_proto::terminals::ended::ENDED_SHELL_MAX_CHARS;
use natamux_proto::terminals::history::{
    TERMINAL_HISTORY_BYTES, TERMINAL_SCROLLBACK_MAX_LINES, TERMINAL_SCROLLBACK_MIN_LINES,
};
use natamux_proto::terminals::messages::{ACK_EVERY_BYTES, MAX_SIZE, SILENT_CLOSE};

const TERMINAL_TS: &str = include_str!("../../../packages/core/src/terminals/terminal.ts");
const PORTS_TS: &str = include_str!("../../../packages/core/src/terminals/ports.ts");

/// The right side of `export const NAME = <value>`, as a number: `64 * 1024` and `10_000` work.
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
fn terminal_limits_match_core() {
    assert_eq!(
        number(TERMINAL_TS, "SILENT_CLOSE_MS"),
        SILENT_CLOSE.as_millis() as u64
    );
    assert_eq!(number(TERMINAL_TS, "ACK_EVERY_BYTES"), ACK_EVERY_BYTES);
    assert_eq!(
        number(TERMINAL_TS, "TERMINAL_HISTORY_BYTES"),
        TERMINAL_HISTORY_BYTES as u64
    );
    assert_eq!(
        number(TERMINAL_TS, "TERMINAL_SCROLLBACK_MIN_LINES"),
        TERMINAL_SCROLLBACK_MIN_LINES as u64
    );
    assert_eq!(
        number(TERMINAL_TS, "TERMINAL_SCROLLBACK_MAX_LINES"),
        TERMINAL_SCROLLBACK_MAX_LINES as u64
    );
    assert_eq!(
        number(TERMINAL_TS, "ENDED_SHELL_MAX_CHARS"),
        ENDED_SHELL_MAX_CHARS as u64
    );
    assert!(TERMINAL_TS.contains("const BYTES_PER_CELL = 16"));
    assert!(TERMINAL_TS.contains(&format!("const MAX_SIZE = {MAX_SIZE}")));
}

#[test]
fn port_blocks_match_core() {
    assert_eq!(number(PORTS_TS, "PORT_BASE"), u64::from(PORT_BASE));
    assert_eq!(
        number(PORTS_TS, "PORTS_PER_TASK"),
        u64::from(PORTS_PER_TASK)
    );
    assert!(PORTS_TS.contains("const BLOCKS = 1000"));
}
