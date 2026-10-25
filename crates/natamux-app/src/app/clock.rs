//! The views' clock, in Unix milliseconds. `--shot` stops it at the fake daemon's time, so the
//! ages in a screenshot do not change from one CI run to the next.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, Task};

use crate::components::motion;

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

/// The timer of `follow_labels`, with the time it is set for in Unix ms: a new time drops it.
#[derive(Default)]
pub struct LabelClock {
    pub at: Option<i64>,
    pub timer: Option<Task<()>>,
}

impl LabelClock {
    /// Keeps the timer when the next change is still at `at`.
    pub fn set_for(&mut self, at: Option<i64>) {
        if self.at != at {
            self.at = at;
            self.timer = None;
        }
    }
}

/// Keeps `clock` at one timer that runs `fired` when the next time label of a view changes, in
/// `wait` ms. A label that changes by the clock alone gets no data change to draw it. No timer
/// runs without focus: the window draws every view again when focus comes back. `fired` clears
/// the clock.
pub fn follow_labels<V: 'static>(
    clock: &mut Option<Task<()>>,
    wait: Option<i64>,
    cx: &mut Context<V>,
    fired: fn(&mut V, &mut Context<V>),
) {
    if !motion::window_active(cx) {
        *clock = None;
        return;
    }
    if clock.is_some() {
        return;
    }
    let Some(wait) = wait else {
        return;
    };
    *clock = Some(cx.spawn(async move |view, cx| {
        let wait = Duration::from_millis(u64::try_from(wait).unwrap_or(0));
        cx.background_executor().timer(wait).await;
        view.update(cx, |view, cx| fired(view, cx)).ok();
    }));
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
