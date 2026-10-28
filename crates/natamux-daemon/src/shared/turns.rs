//! A lock that gives turns in the order they were asked for. A `Mutex` lets the thread that just
//! let go take it again at once, so a shell that floods kept the turn from a quiet one for
//! hundreds of ms. With several lanes, up to that many turns are held at once, still started in
//! ticket order: a later ticket never starts before an earlier one.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

#[derive(Debug)]
pub struct Turns {
    queue: Mutex<Queue>,
    moved: Condvar,
    lanes: u64,
}

#[derive(Debug, Default)]
struct Queue {
    /// The next ticket to give, the tickets started so far, and the turns held now.
    next: u64,
    started: u64,
    held: u64,
}

/// The turn is held until this drops, also when a panic unwinds.
pub struct Turn<'a>(&'a Turns);

/// One lane: one turn at a time.
impl Default for Turns {
    fn default() -> Turns {
        Turns::with_lanes(1)
    }
}

impl Turns {
    /// Up to `lanes` turns at once; 0 counts as 1.
    pub fn with_lanes(lanes: usize) -> Turns {
        Turns {
            queue: Mutex::default(),
            moved: Condvar::new(),
            lanes: lanes.max(1) as u64,
        }
    }

    pub fn take(&self) -> Turn<'_> {
        let mut queue = self.lock();
        let ticket = queue.next;
        queue.next += 1;
        while queue.started != ticket || queue.held >= self.lanes {
            queue = self
                .moved
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
        queue.started += 1;
        queue.held += 1;
        let free = queue.held < self.lanes;
        drop(queue);
        if free {
            // The next ticket may start in a free lane now.
            self.moved.notify_all();
        }
        Turn(self)
    }

    /// The turns held or asked for.
    pub fn asked(&self) -> u64 {
        let queue = self.lock();
        queue.next - queue.started + queue.held
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.lock().held -= 1;
        self.0.moved.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::Turns;

    fn wait_until(done: impl Fn() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    // Regression: with a Mutex, a shell that let go and asked again at once got the turn back
    // before the shell that waited.
    #[test]
    fn one_that_asks_again_waits_behind_one_that_asked_before() {
        let turns = Turns::default();
        let order = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            let first = turns.take();
            scope.spawn(|| {
                let _turn = turns.take();
                order.lock().unwrap().push("quiet");
            });
            wait_until(|| turns.asked() == 2);
            drop(first);
            let _again = turns.take();
            order.lock().unwrap().push("flood");
        });
        assert_eq!(*order.lock().unwrap(), ["quiet", "flood"]);
    }

    #[test]
    fn turns_go_in_the_order_they_were_asked_for() {
        let turns = Turns::default();
        let order = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            let first = turns.take();
            for name in ["a", "b", "c"] {
                let (turns, order) = (&turns, &order);
                let asked = turns.asked();
                scope.spawn(move || {
                    let _turn = turns.take();
                    order.lock().unwrap().push(name);
                });
                wait_until(|| turns.asked() == asked + 1);
            }
            drop(first);
        });
        assert_eq!(*order.lock().unwrap(), ["a", "b", "c"]);
        assert_eq!(turns.asked(), 0);
    }

    #[test]
    fn two_lanes_hold_two_turns_at_once_and_the_third_waits_for_one_to_end() {
        let turns = Turns::with_lanes(2);
        let order = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            let first = turns.take();
            // A second turn starts while the first is held.
            let second = turns.take();
            assert_eq!(turns.asked(), 2);
            scope.spawn(|| {
                let _turn = turns.take();
                order.lock().unwrap().push("third");
            });
            wait_until(|| turns.asked() == 3);
            std::thread::sleep(Duration::from_millis(20));
            assert!(order.lock().unwrap().is_empty());
            drop(first);
            wait_until(|| order.lock().unwrap().len() == 1);
            drop(second);
        });
        assert_eq!(turns.asked(), 0);
    }

    #[test]
    fn a_later_ticket_never_starts_before_an_earlier_one() {
        let turns = Turns::with_lanes(2);
        let started = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            let first = turns.take();
            let second = turns.take();
            for name in ["a", "b"] {
                let (turns, started) = (&turns, &started);
                let asked = turns.asked();
                scope.spawn(move || {
                    let turn = turns.take();
                    started.lock().unwrap().push(name);
                    // Held until both started, so "b" can only start beside "a" or after it.
                    wait_until(|| started.lock().unwrap().len() == 2);
                    drop(turn);
                });
                wait_until(|| turns.asked() == asked + 1);
            }
            // One lane frees: only "a", the earlier ticket, may take it.
            drop(second);
            wait_until(|| !started.lock().unwrap().is_empty());
            assert_eq!(*started.lock().unwrap(), ["a"]);
            drop(first);
        });
        assert_eq!(*started.lock().unwrap(), ["a", "b"]);
    }

    #[test]
    fn a_panic_in_a_turn_gives_the_turn_on() {
        let turns = Turns::default();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _turn = turns.take();
            panic!("a bug in one shell");
        }));
        assert!(panicked.is_err());
        let _turn = turns.take();
        assert_eq!(turns.asked(), 1);
    }
}
