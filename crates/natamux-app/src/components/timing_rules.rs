//! Timing of the skeleton (`hooks/useDelayedFlag.ts`, docs/ui-spec.md 4.20) and of a toast's
//! pause on hover (`ui/Toaster.tsx`). Times are ms on one clock; nothing reads the clock, so it is
//! tested alone.

/// A skeleton shows only after this much loading, so a fast load does not flash.
pub const SKELETON_DELAY_MS: u64 = 200;
/// A shown skeleton stays at least this long, so it does not blink when the data comes.
pub const SKELETON_MIN_MS: u64 = 300;

/// Turns a loading flag into the "show the skeleton" flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DelayedFlag {
    loading_since: Option<u64>,
    shown_at: Option<u64>,
}

impl DelayedFlag {
    /// Whether the skeleton shows at `now` while `loading`, and how many ms to wait before asking
    /// again because the answer will change then.
    pub fn poll(&mut self, loading: bool, now: u64) -> (bool, Option<u64>) {
        if loading {
            let since = *self.loading_since.get_or_insert(now);
            if self.shown_at.is_some() {
                return (true, None);
            }
            let waited = now.saturating_sub(since);
            if waited >= SKELETON_DELAY_MS {
                self.shown_at = Some(now);
                return (true, None);
            }
            return (false, Some(SKELETON_DELAY_MS - waited));
        }
        self.loading_since = None;
        let Some(at) = self.shown_at else {
            return (false, None);
        };
        let shown_for = now.saturating_sub(at);
        if shown_for >= SKELETON_MIN_MS {
            self.shown_at = None;
            (false, None)
        } else {
            (true, Some(SKELETON_MIN_MS - shown_for))
        }
    }
}

/// A paused toast keeps at least this much time, so it does not vanish as the pointer leaves.
pub const TOAST_RESUME_MIN_MS: u64 = 1000;

/// The timer of one toast: it runs, pauses while the pointer or focus is on the toast, and resumes
/// with the time that was left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToastClock {
    /// None: the toast stays until dismissed.
    left: Option<u64>,
    running_since: Option<u64>,
}

impl ToastClock {
    pub fn start(duration_ms: Option<u32>, now: u64) -> Self {
        Self {
            left: duration_ms.map(u64::from),
            running_since: Some(now),
        }
    }

    pub fn pause(&mut self, now: u64) {
        if let (Some(left), Some(since)) = (self.left, self.running_since.take()) {
            self.left = Some(
                left.saturating_sub(now.saturating_sub(since))
                    .max(TOAST_RESUME_MIN_MS),
            );
        }
    }

    pub fn resume(&mut self, now: u64) {
        if self.running_since.is_none() {
            self.running_since = Some(now);
        }
    }

    /// When the toast goes; None while paused or when it stays.
    pub fn due_at(&self) -> Option<u64> {
        Some(self.running_since? + self.left?)
    }

    pub fn is_due(&self, now: u64) -> bool {
        self.due_at().is_some_and(|due| now >= due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_show_a_skeleton_for_a_fast_load() {
        let mut flag = DelayedFlag::default();
        assert_eq!(flag.poll(true, 0), (false, Some(200)));
        assert_eq!(flag.poll(true, 150), (false, Some(50)));
        assert_eq!(flag.poll(false, 180), (false, None));
        // A new load waits the full delay again.
        assert_eq!(flag.poll(true, 1000), (false, Some(200)));
    }

    #[test]
    fn shows_the_skeleton_after_200_ms_of_loading() {
        let mut flag = DelayedFlag::default();
        flag.poll(true, 0);
        assert_eq!(flag.poll(true, 200), (true, None));
        assert_eq!(flag.poll(true, 900), (true, None));
    }

    #[test]
    fn keeps_a_shown_skeleton_for_at_least_300_ms() {
        let mut flag = DelayedFlag::default();
        flag.poll(true, 0);
        flag.poll(true, 250);
        assert_eq!(flag.poll(false, 300), (true, Some(250)));
        assert_eq!(flag.poll(false, 549), (true, Some(1)));
        assert_eq!(flag.poll(false, 550), (false, None));
    }

    #[test]
    fn hides_at_once_when_the_load_ended_after_the_minimum() {
        let mut flag = DelayedFlag::default();
        flag.poll(true, 0);
        flag.poll(true, 200);
        assert_eq!(flag.poll(false, 900), (false, None));
    }

    #[test]
    fn a_toast_goes_after_its_duration() {
        let clock = ToastClock::start(Some(5000), 100);
        assert_eq!(clock.due_at(), Some(5100));
        assert!(!clock.is_due(5099));
        assert!(clock.is_due(5100));
    }

    #[test]
    fn a_toast_without_a_duration_stays() {
        let clock = ToastClock::start(None, 0);
        assert_eq!(clock.due_at(), None);
        assert!(!clock.is_due(u64::MAX));
    }

    #[test]
    fn pausing_keeps_the_time_left_and_resumes_from_there() {
        let mut clock = ToastClock::start(Some(5000), 0);
        clock.pause(3000);
        assert_eq!(clock.due_at(), None);
        assert!(!clock.is_due(99_000));
        clock.resume(10_000);
        assert_eq!(clock.due_at(), Some(12_000));
    }

    #[test]
    fn a_resumed_toast_keeps_at_least_one_second() {
        let mut clock = ToastClock::start(Some(5000), 0);
        clock.pause(4900);
        clock.resume(6000);
        assert_eq!(clock.due_at(), Some(7000));
    }

    #[test]
    fn pausing_twice_or_resuming_twice_changes_nothing() {
        let mut clock = ToastClock::start(Some(5000), 0);
        clock.pause(1000);
        clock.pause(2000);
        clock.resume(3000);
        clock.resume(4000);
        assert_eq!(clock.due_at(), Some(7000));
    }
}
