//! A lock that gives turns in the order they were asked for. A `Mutex` lets the thread that just
//! let go take it again at once, so a shell that floods kept the turn from a quiet one for
//! hundreds of ms.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub struct Turns {
    queue: Mutex<Queue>,
    moved: Condvar,
}

#[derive(Debug, Default)]
struct Queue {
    next: u64,
    serving: u64,
}

/// The turn is held until this drops, also when a panic unwinds.
pub struct Turn<'a>(&'a Turns);

impl Turns {
    pub fn take(&self) -> Turn<'_> {
        let mut queue = self.lock();
        let ticket = queue.next;
        queue.next += 1;
        while queue.serving != ticket {
            queue = self
                .moved
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
        Turn(self)
    }

    /// The turns held or asked for.
    pub fn asked(&self) -> u64 {
        let queue = self.lock();
        queue.next - queue.serving
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.lock().serving += 1;
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
