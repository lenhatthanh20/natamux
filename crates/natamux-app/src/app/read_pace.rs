//! The pace of one kind of read that follows change events. An agent that writes files sends
//! many changes a second: one read runs at a time, at most one starts a second, and the asks
//! made meanwhile become one read after it, so the last change is never lost.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// At most one read of a kind starts in this time.
pub const READ_GAP_MS: i64 = 1000;

/// The clock of the pace, in ms. Not the views' clock: `--shot` stops that one, and a stopped
/// clock would make every read wait for ever.
pub fn pace_ms() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let since = START.get_or_init(Instant::now).elapsed();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}

/// The wait of `Pace::After`, for a timer.
pub fn wait_of(ms: i64) -> Duration {
    Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

#[derive(Debug, Default)]
pub struct ReadPace {
    running: bool,
    started_ms: Option<i64>,
    /// A read was asked for meanwhile; `true` when one of the asks was fresh.
    again: Option<bool>,
    /// A timer will ask again.
    timer: bool,
}

/// What to do with an ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Start the read.
    Now,
    /// Ask again after this many ms, with `ReadPace::timer_fired`.
    After(i64),
    /// A read or a timer that runs now asks again when it ends.
    Wait,
}

impl ReadPace {
    /// A fresh ask (the user's Refresh) does not wait for the gap, only for the read that runs.
    pub fn ask(&mut self, fresh: bool, now_ms: i64) -> Pace {
        let soon = match self.started_ms {
            Some(started) if !fresh => READ_GAP_MS - (now_ms - started),
            _ => 0,
        };
        if self.running || soon > 0 {
            self.again = Some(fresh || self.again == Some(true));
            if self.running || self.timer {
                return Pace::Wait;
            }
            self.timer = true;
            return Pace::After(soon);
        }
        self.running = true;
        self.started_ms = Some(now_ms);
        Pace::Now
    }

    /// The read ended: ask again with this freshness, unless a timer will.
    pub fn ended(&mut self) -> Option<bool> {
        self.running = false;
        if self.timer {
            return None;
        }
        self.again.take()
    }

    /// Back to "nothing read": a task shown again reads at once, and a read that was running
    /// when the task left (its answer was dropped) no longer blocks the next one.
    pub fn reset(&mut self) {
        *self = ReadPace::default();
    }

    /// The timer of `Pace::After` fired: ask again with this freshness.
    pub fn timer_fired(&mut self) -> Option<bool> {
        self.timer = false;
        self.again.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_ask_alone_reads_at_once() {
        let mut pace = ReadPace::default();
        assert_eq!(pace.ask(false, 0), Pace::Now);
        assert_eq!(pace.ended(), None);
        assert_eq!(pace.ask(false, 5000), Pace::Now);
    }

    #[test]
    fn twenty_asks_in_a_second_make_one_read_and_one_after_it() {
        let mut pace = ReadPace::default();
        let mut starts = 0;
        let mut timers = Vec::new();
        for step in 0..20 {
            match pace.ask(false, step * 50) {
                Pace::Now => starts += 1,
                Pace::After(wait) => timers.push(wait),
                Pace::Wait => {}
            }
            if step == 2 {
                // The first read ends quickly: the asks meanwhile wait for the gap.
                assert_eq!(pace.ended(), Some(false));
                assert_eq!(pace.ask(false, 100), Pace::After(900));
            }
        }
        assert_eq!((starts, timers), (1, Vec::new()));
        // The last ask is not lost: the timer reads once at the end of the second.
        assert_eq!(pace.timer_fired(), Some(false));
        assert_eq!(pace.ask(false, 1000), Pace::Now);
        assert_eq!(pace.ended(), None);
    }

    #[test]
    fn the_asks_while_a_read_runs_make_one_read_after_it() {
        let mut pace = ReadPace::default();
        assert_eq!(pace.ask(false, 0), Pace::Now);
        assert_eq!(pace.ask(false, 1500), Pace::Wait);
        assert_eq!(pace.ask(false, 1600), Pace::Wait);
        assert_eq!(pace.ended(), Some(false));
        assert_eq!(pace.ask(false, 2000), Pace::Now);
    }

    #[test]
    fn a_fresh_ask_skips_the_gap_and_makes_the_next_read_fresh() {
        let mut pace = ReadPace::default();
        assert_eq!(pace.ask(false, 0), Pace::Now);
        assert_eq!(pace.ask(true, 10), Pace::Wait);
        assert_eq!(pace.ask(false, 20), Pace::Wait);
        assert_eq!(pace.ended(), Some(true));
        assert_eq!(pace.ask(true, 30), Pace::Now);
    }

    #[test]
    fn a_timer_that_fires_while_a_read_runs_leaves_the_ask_to_its_end() {
        let mut pace = ReadPace::default();
        assert_eq!(pace.ask(false, 0), Pace::Now);
        assert_eq!(pace.ended(), None);
        assert_eq!(pace.ask(false, 100), Pace::After(900));
        assert_eq!(pace.timer_fired(), Some(false));
        assert_eq!(pace.ask(false, 1000), Pace::Now);
        assert_eq!(pace.ask(false, 1100), Pace::Wait);
        assert_eq!(pace.ended(), Some(false));
        assert_eq!(pace.ask(false, 1200), Pace::After(800));
    }

    #[test]
    fn a_reset_reads_at_once_also_inside_the_gap_or_while_a_read_ran() {
        let mut pace = ReadPace::default();
        assert_eq!(pace.ask(false, 0), Pace::Now);
        assert_eq!(pace.ask(false, 10), Pace::Wait);
        pace.reset();
        assert_eq!(pace.ask(false, 20), Pace::Now);
    }
}
