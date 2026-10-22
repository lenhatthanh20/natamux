//! The views' clock, in Unix milliseconds. `--shot` stops it at the fake daemon's time, so the
//! ages in a screenshot do not change from one CI run to the next.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 0: the real clock.
static STOPPED_AT: AtomicI64 = AtomicI64::new(0);

pub fn now_ms() -> i64 {
    let stopped = STOPPED_AT.load(Ordering::Relaxed);
    if stopped != 0 {
        return stopped;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

pub fn stop_at(ms: i64) {
    STOPPED_AT.store(ms, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stopped_clock_stays_put() {
        assert!(now_ms() > 1_700_000_000_000);
        stop_at(1_000);
        assert_eq!(now_ms(), 1_000);
        stop_at(0);
        assert!(now_ms() > 1_700_000_000_000);
    }
}
