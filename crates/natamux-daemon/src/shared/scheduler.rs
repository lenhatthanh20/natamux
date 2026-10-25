//! Timers, as `setTimeout` in the Node daemon. A job runs on the scheduler's own thread, never
//! inside the call that set it, so it may take any lock. Tests use a manual clock and move it on.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce() + Send>;

/// Names a timer, to cancel it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

#[derive(Default)]
struct Jobs {
    by_time: BTreeMap<(u64, u64), Job>,
    times: HashMap<u64, u64>,
    stopped: bool,
}

pub struct Scheduler {
    jobs: Mutex<Jobs>,
    wake: Condvar,
    next_id: AtomicU64,
    clock: Clock,
}

enum Clock {
    Real(Instant),
    Manual(AtomicU64),
}

impl Scheduler {
    /// Real time, with a thread that runs the jobs.
    pub fn real() -> Arc<Scheduler> {
        let scheduler = Arc::new(Scheduler::with(Clock::Real(Instant::now())));
        let runner = Arc::clone(&scheduler);
        std::thread::spawn(move || runner.run());
        scheduler
    }

    /// A clock that only moves when `advance` is called.
    pub fn manual() -> Arc<Scheduler> {
        Arc::new(Scheduler::with(Clock::Manual(AtomicU64::new(0))))
    }

    fn with(clock: Clock) -> Scheduler {
        Scheduler {
            jobs: Mutex::new(Jobs::default()),
            wake: Condvar::new(),
            next_id: AtomicU64::new(1),
            clock,
        }
    }

    /// Milliseconds since the scheduler was made.
    pub fn now(&self) -> u64 {
        match &self.clock {
            Clock::Real(start) => start.elapsed().as_millis() as u64,
            Clock::Manual(now) => now.load(Ordering::SeqCst),
        }
    }

    /// Runs `job` once `delay` milliseconds have passed. A delay of 0 runs it as soon as possible,
    /// after the current call.
    pub fn after(&self, delay: u64, job: impl FnOnce() + Send + 'static) -> TimerId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let at = self.now().saturating_add(delay);
        let mut jobs = self.lock();
        jobs.by_time.insert((at, id), Box::new(job));
        jobs.times.insert(id, at);
        drop(jobs);
        self.wake.notify_all();
        TimerId(id)
    }

    /// Runs a slow job, such as a `ps`, on a thread of its own, so no timer waits for it. A manual
    /// clock runs it as a job that is due now, so tests stay in step.
    pub fn off_thread(&self, job: impl FnOnce() + Send + 'static) {
        match &self.clock {
            Clock::Real(_) => {
                std::thread::spawn(job);
            }
            Clock::Manual(_) => {
                self.after(0, job);
            }
        }
    }

    pub fn cancel(&self, timer: TimerId) {
        let mut jobs = self.lock();
        if let Some(at) = jobs.times.remove(&timer.0) {
            jobs.by_time.remove(&(at, timer.0));
        }
    }

    /// Cancels the timer in `slot`, if any, and empties it.
    pub fn clear(&self, slot: &mut Option<TimerId>) {
        if let Some(timer) = slot.take() {
            self.cancel(timer);
        }
    }

    pub fn is_pending(&self, timer: TimerId) -> bool {
        self.lock().times.contains_key(&timer.0)
    }

    /// Manual clock: moves time on by `ms`, running every job that falls due on the way, in order,
    /// each at its own time. Jobs set by jobs run too when they fall in the window.
    pub fn advance(&self, ms: u64) {
        let Clock::Manual(now) = &self.clock else {
            return;
        };
        let end = now.load(Ordering::SeqCst).saturating_add(ms);
        while let Some((at, job)) = self.pop_due(end) {
            now.fetch_max(at, Ordering::SeqCst);
            job();
        }
        now.store(end, Ordering::SeqCst);
    }

    /// Stops the runner thread of a real scheduler. Jobs not run yet are dropped.
    pub fn stop(&self) {
        let mut jobs = self.lock();
        jobs.stopped = true;
        jobs.by_time.clear();
        jobs.times.clear();
        drop(jobs);
        self.wake.notify_all();
    }

    fn pop_due(&self, until: u64) -> Option<(u64, Job)> {
        let mut jobs = self.lock();
        let (&(at, id), _) = jobs.by_time.iter().next()?;
        if at > until {
            return None;
        }
        jobs.times.remove(&id);
        jobs.by_time.remove(&(at, id)).map(|job| (at, job))
    }

    fn run(&self) {
        loop {
            let mut jobs = self.lock();
            if jobs.stopped {
                return;
            }
            let now = self.now();
            match jobs.by_time.keys().next().copied() {
                Some((at, id)) if at <= now => {
                    jobs.times.remove(&id);
                    let job = jobs.by_time.remove(&(at, id));
                    drop(jobs);
                    if let Some(job) = job {
                        job();
                    }
                }
                Some((at, _)) => {
                    let wait = Duration::from_millis(at - now);
                    drop(self.wake.wait_timeout(jobs, wait));
                }
                None => drop(self.wake.wait(jobs)),
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Jobs> {
        // A job that panicked (a bug) must not stop every other timer.
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_jobs_in_order_of_time_each_at_its_time() {
        let scheduler = Scheduler::manual();
        let seen = Arc::new(Mutex::new(Vec::new()));
        for (delay, name) in [(10, "b"), (0, "a"), (10, "c"), (30, "d")] {
            let seen = Arc::clone(&seen);
            let clock = Arc::clone(&scheduler);
            scheduler.after(delay, move || {
                seen.lock().unwrap().push((name, clock.now()))
            });
        }
        scheduler.advance(10);
        assert_eq!(*seen.lock().unwrap(), [("a", 0), ("b", 10), ("c", 10)]);
        scheduler.advance(25);
        assert_eq!(seen.lock().unwrap().last(), Some(&("d", 30)));
        assert_eq!(scheduler.now(), 35);
    }

    #[test]
    fn a_cancelled_job_never_runs_and_a_job_may_set_another() {
        let scheduler = Scheduler::manual();
        let ran = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&ran);
        let timer = scheduler.after(5, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert!(scheduler.is_pending(timer));
        scheduler.cancel(timer);
        assert!(!scheduler.is_pending(timer));
        let again = Arc::clone(&scheduler);
        let counter = Arc::clone(&ran);
        scheduler.after(1, move || {
            again.after(1, move || {
                counter.fetch_add(10, Ordering::SeqCst);
            });
        });
        scheduler.advance(10);
        assert_eq!(ran.load(Ordering::SeqCst), 10);
    }

    /// The `ps` of one shell (here 500 ms) must not hold the 4 ms flush of another.
    #[test]
    fn a_slow_job_off_the_thread_does_not_hold_the_flush_of_another_shell() {
        let scheduler = Scheduler::real();
        let (slow_done, slow) = std::sync::mpsc::channel();
        scheduler.off_thread(move || {
            std::thread::sleep(Duration::from_millis(500));
            slow_done.send(()).unwrap();
        });
        let (flushed, flush) = std::sync::mpsc::channel();
        let start = Instant::now();
        scheduler.after(4, move || flushed.send(Instant::now()).unwrap());
        let at = flush.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(at.duration_since(start) < Duration::from_millis(250));
        slow.recv_timeout(Duration::from_secs(5)).unwrap();
        scheduler.stop();
    }

    #[test]
    fn a_manual_clock_runs_a_job_off_the_thread_on_the_next_advance() {
        let scheduler = Scheduler::manual();
        let ran = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&ran);
        scheduler.off_thread(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        scheduler.advance(0);
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_real_clock_runs_jobs_on_its_thread() {
        let scheduler = Scheduler::real();
        let (done, wait) = std::sync::mpsc::channel();
        scheduler.after(5, move || done.send(()).unwrap());
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        scheduler.stop();
    }
}
