#![cfg(unix)]

use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::{CapturedBytes, capture_bytes};

fn sh(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

fn emit(out: usize, err: usize) -> String {
    format!("head -c {out} /dev/zero; head -c {err} /dev/zero >&2")
}

#[test]
fn limit_counts_each_stream_alone() {
    let ran = capture_bytes(sh(&emit(600, 600)), Duration::from_secs(30), 1000);
    assert!(!ran.overflow);
    assert_eq!(ran.stdout.len(), 600);
    assert_eq!(ran.stderr.len(), 600);
    let ran = capture_bytes(sh(&emit(1100, 0)), Duration::from_secs(30), 1000);
    assert!(ran.overflow);
}

#[test]
fn kills_a_child_that_closes_its_pipes_and_sleeps() {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let ran = capture_bytes(
            sh("exec >&- 2>&-; sleep 30"),
            Duration::from_millis(200),
            1000,
        );
        let _ = sender.send(ran);
    });
    let ran: CapturedBytes = receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("capture_bytes did not return after the timeout"));
    assert!(ran.timed_out);
}
