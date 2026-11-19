use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::executor::block_on;

use super::*;

fn heard(events: &mut UnboundedReceiver<TerminalEvent>) -> Vec<TerminalEvent> {
    let mut heard = Vec::new();
    while let Ok(event) = events.try_recv() {
        heard.push(event);
    }
    heard
}

#[test]
fn a_terminal_with_no_flood_gets_its_screen_and_an_ended_feed() {
    let fake = FakeDaemon::default();
    let mut events = fake.terminal("T-0146", "12");
    assert_eq!(heard(&mut events).len(), 2);
    assert_eq!(block_on(events.next()), None);
}

#[test]
fn a_flooded_terminal_prints_lines_after_its_screen_until_the_flood_ends() {
    let fake = FakeDaemon::default();
    // FLOODS is shared by the test threads: a task no other test opens.
    let flood = fake.flood("T-flood-test", "1", 1_000);
    let mut events = fake.terminal("T-flood-test", "1");
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(flood.sent(), 0, "no line before printing is on");
    flood.set_printing(true);
    let started = Instant::now();
    while flood.sent() < 3 && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(flood);
    let heard = heard(&mut events);
    assert_eq!(
        heard.first(),
        Some(&TerminalEvent::Status(TerminalStatus::Open))
    );
    let mut offset = 0;
    for event in &heard[1..] {
        let TerminalEvent::Output { bytes, end } = event else {
            panic!("not output: {event:?}");
        };
        offset += bytes.len() as u64;
        assert_eq!(*end, Some(offset), "the offsets follow each other");
    }
    assert!(heard.len() >= 5, "open, the screen and 3 lines: {heard:?}");
    assert_eq!(
        block_on(events.next()),
        None,
        "the feed ends with the flood"
    );
}

#[test]
fn an_echoed_terminal_sends_its_input_back_after_its_screen() {
    let fake = FakeDaemon::default();
    let echo = fake.echo("T-echo-test", "1");
    let mut handle = fake.open_terminal("T-echo-test", "1");
    let opened = heard(&mut handle.events);
    let Some(TerminalEvent::Output { end: Some(end), .. }) = opened.last() else {
        panic!("no screen: {opened:?}");
    };
    handle.input("a");
    let echoed = block_on(handle.events.next());
    assert_eq!(
        echoed,
        Some(TerminalEvent::Output {
            bytes: b"a".to_vec(),
            end: Some(end + 1),
        })
    );
    drop(echo);
    assert_eq!(
        block_on(handle.events.next()),
        None,
        "the feed ends with the echo"
    );
}

#[test]
fn input_to_a_terminal_with_no_echo_goes_nowhere() {
    let fake = FakeDaemon::default();
    let mut handle = fake.open_terminal("T-0146", "12");
    handle.input("a");
    assert_eq!(heard(&mut handle.events).len(), 2);
    assert_eq!(block_on(handle.events.next()), None);
}
