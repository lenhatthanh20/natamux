//! `describe('a hidden client')`.

use std::collections::HashMap;

use natamux_daemon::terminals::limits::{
    HIDDEN_BUDGET_BYTES, HIDDEN_BUDGET_WINDOW_MS, OUTPUT_FRAME_BYTES, RESYNC_MAX_WAIT_MS,
};
use natamux_daemon::terminals::shells::Detach;
use natamux_proto::terminals::messages::ServerMessage;

use super::util::{TASK, Watcher, adv, catch_ups, flood, frame_of, snapshot_msg};
use crate::support::{FakePty, Harness, bytes_of, controls, setup};

const FRAME: u64 = OUTPUT_FRAME_BYTES as u64;

struct Hidden {
    h: Harness,
    watchers: Vec<Watcher>,
    _detaches: Vec<Detach>,
    hidden_at: u64,
}

impl Hidden {
    fn new(terminals: &[&str]) -> Hidden {
        let h = setup();
        let (watchers, detaches): (Vec<_>, Vec<_>) = terminals
            .iter()
            .map(|terminal| Watcher::attach(&h, terminal, false))
            .unzip();
        adv(&h, &[], 0);
        let hidden_at = h.scheduler.now();
        Hidden {
            h,
            watchers,
            _detaches: detaches,
            hidden_at,
        }
    }

    fn adv(&self, ms: u64) {
        let all: Vec<&Watcher> = self.watchers.iter().collect();
        adv(&self.h, &all, ms);
    }

    fn flood(&self, pty: &FakePty, ms: u64, bytes: usize) {
        let all: Vec<&Watcher> = self.watchers.iter().collect();
        flood(&self.h, &all, pty, ms, bytes);
    }

    fn show(&self, index: usize, visible: bool) {
        let watcher = &self.watchers[index];
        let terminal = (index + 1).to_string();
        self.h
            .service
            .visible(TASK, &terminal, &watcher.client, visible, None);
    }
}

fn one() -> (Hidden, FakePty) {
    let hidden = Hidden::new(&["1"]);
    let pty = hidden.h.pty(0);
    (hidden, pty)
}

fn over_budget(pty: &FakePty) {
    for index in 0..=(HIDDEN_BUDGET_BYTES / FRAME) as usize {
        pty.emit(frame_of(index));
    }
}

#[test]
fn stays_current_through_10_minutes_of_a_status_line_redrawn_at_2_kb_s() {
    let (hidden, pty) = one();
    let redraw = format!("\r\x1b[2K{}", "status ".repeat(27));
    let mut printed = String::new();

    for _ in 0..6000 {
        pty.emit(&redraw);
        printed.push_str(&redraw);
        hidden.adv(100);
    }

    let watching = &hidden.watchers[0];
    assert_eq!(watching.output(), printed);
    assert_eq!(catch_ups(&watching.frames()), []);
}

// Less makes a terminal hidden during a 200 KB/s stream catch up slowly when shown (E2E).
#[test]
fn gets_256_kib_per_second() {
    assert_eq!(HIDDEN_BUDGET_BYTES, 256 * 1024);
    assert_eq!(HIDDEN_BUDGET_WINDOW_MS, 1000);
}

#[test]
fn takes_its_whole_budget_in_one_window_and_a_burst_split_by_a_window_border() {
    let (hidden, pty) = one();
    let watching = &hidden.watchers[0];
    let budget = (HIDDEN_BUDGET_BYTES / FRAME) as usize;
    let full = 3 * budget * OUTPUT_FRAME_BYTES;

    for index in 0..budget {
        pty.emit(frame_of(index));
    }
    // A full window just before a border, and a full one just after it.
    hidden.adv(2 * HIDDEN_BUDGET_WINDOW_MS - 100);
    for index in 0..budget {
        pty.emit(frame_of(budget + index));
    }
    hidden.adv(100);
    for index in 0..budget {
        pty.emit(frame_of(2 * budget + index));
    }
    hidden.adv(0);
    assert_eq!(bytes_of(&watching.frames()).len(), full);

    // One byte past the budget of this window: behind, and not caught up while hidden.
    pty.emit("!");
    hidden.adv(RESYNC_MAX_WAIT_MS);
    assert_eq!(bytes_of(&watching.frames()).len(), full);
    assert_eq!(catch_ups(&watching.frames()), []);
}

#[test]
fn is_not_caught_up_while_hidden_and_is_once_the_page_is_back() {
    let (hidden, pty) = one();
    let watching = &hidden.watchers[0];
    over_budget(&pty);

    hidden.adv(5 * RESYNC_MAX_WAIT_MS);
    assert_eq!(catch_ups(&watching.frames()), []);

    // Without cols and rows: the page came back, which is visible enough.
    hidden.show(0, true);
    assert_eq!(
        catch_ups(&watching.frames()),
        [ServerMessage::Resume {
            offset: HIDDEN_BUDGET_BYTES
        }]
    );
    assert_eq!(
        controls(&watching.frames()).last(),
        Some(&ServerMessage::Replayed {
            offset: HIDDEN_BUDGET_BYTES + FRAME
        })
    );
}

#[test]
fn waits_again_when_it_hides_before_its_catch_up_starts() {
    let (hidden, pty) = one();
    let watching = &hidden.watchers[0];
    over_budget(&pty);

    hidden.flood(&pty, 500, 100);
    hidden.show(0, true);
    hidden.flood(&pty, 100, 100);
    hidden.show(0, false);
    hidden.flood(&pty, 2 * RESYNC_MAX_WAIT_MS, 100);
    assert_eq!(catch_ups(&watching.frames()), []);

    // Behind for longer than RESYNC_MAX_WAIT_MS: caught up at once, flood or not.
    hidden.show(0, true);
    assert_eq!(
        catch_ups(&watching.frames()),
        [ServerMessage::Resume {
            offset: HIDDEN_BUDGET_BYTES
        }]
    );
}

#[test]
fn gives_each_of_several_hidden_clients_at_most_hidden_budget_bytes_per_window() {
    let hidden = Hidden::new(&["1", "2", "3", "4"]);

    // A flood in every shell, 640 KB/s each.
    for step in 0..25 {
        for pty in hidden.h.ptys() {
            pty.emit(frame_of(step));
        }
        hidden.adv(100);
    }

    for each in &hidden.watchers {
        let mut windows: HashMap<u64, u64> = HashMap::new();
        for (at, bytes) in each.arrivals() {
            let slot = (at - hidden.hidden_at) / HIDDEN_BUDGET_WINDOW_MS;
            *windows.entry(slot).or_default() += bytes;
        }
        for bytes in windows.values() {
            assert!(*bytes <= HIDDEN_BUDGET_BYTES);
        }
        assert!(bytes_of(&each.frames()).len() < 25 * OUTPUT_FRAME_BYTES);
        assert_eq!(catch_ups(&each.frames()), []);
    }

    // Shown again: what it missed left the ring, so it gets a snapshot.
    hidden.show(0, true);
    hidden.adv(1000);
    let shown = &hidden.watchers[0];
    assert_eq!(catch_ups(&shown.frames()), [snapshot_msg()]);
    assert!(matches!(
        controls(&shown.frames()).last(),
        Some(ServerMessage::Replayed { .. })
    ));
}
