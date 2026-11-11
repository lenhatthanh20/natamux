//! The PATH of the user's login shell, which the desktop app sends after the daemon started. The
//! daemon serves at once; only a program that is spawned waits for it. `set_var` is never used:
//! threads already run, so it is not safe.

use std::process::Command;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

enum State {
    /// Until the deadline; then the inherited PATH stays.
    Pending(Instant),
    Done(Option<String>),
}

/// Not pending is `Done(None)`, so a process that expects no PATH never waits.
pub struct LoginPath {
    state: Mutex<State>,
    ready: Condvar,
}

impl LoginPath {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(State::Done(None)),
            ready: Condvar::new(),
        }
    }

    /// The PATH is coming: `wait` holds for it at most `within`.
    pub fn expect(&self, within: Duration) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) =
            State::Pending(Instant::now() + within);
    }

    /// The first answer wins. None keeps the inherited PATH.
    pub fn set(&self, path: Option<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*state, State::Pending(_)) {
            *state = State::Done(path);
            self.ready.notify_all();
        }
    }

    pub fn wait(&self) -> Option<String> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while let State::Pending(deadline) = *state {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                *state = State::Done(None);
                self.ready.notify_all();
                break;
            }
            state = self
                .ready
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        match &*state {
            State::Done(path) => path.clone(),
            State::Pending(_) => None,
        }
    }
}

impl Default for LoginPath {
    fn default() -> Self {
        Self::new()
    }
}

pub static LOGIN_PATH: LoginPath = LoginPath::new();

pub fn expect(within: Duration) {
    LOGIN_PATH.expect(within);
}

pub fn set(path: Option<String>) {
    LOGIN_PATH.set(path);
}

pub fn wait() -> Option<String> {
    LOGIN_PATH.wait()
}

/// For a program that must see the login PATH (git-lfs and gh from Homebrew). Waits for it.
pub fn apply(command: &mut Command) {
    apply_from(&LOGIN_PATH, command);
}

pub fn apply_from(cell: &LoginPath, command: &mut Command) {
    if let Some(path) = cell.wait() {
        command.env("PATH", path);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const LONG: Duration = Duration::from_secs(30);

    #[test]
    fn not_pending_never_waits() {
        let cell = LoginPath::new();
        let started = Instant::now();
        assert_eq!(cell.wait(), None);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn first_set_wins_and_wait_then_returns_at_once() {
        let cell = LoginPath::new();
        cell.expect(LONG);
        cell.set(Some("/opt/bin:/usr/bin".into()));
        cell.set(Some("/late".into()));
        assert_eq!(cell.wait().as_deref(), Some("/opt/bin:/usr/bin"));
        assert_eq!(cell.wait().as_deref(), Some("/opt/bin:/usr/bin"));
    }

    #[test]
    fn a_waiter_wakes_when_the_path_is_set() {
        let cell = Arc::new(LoginPath::new());
        cell.expect(LONG);
        let waiter = {
            let cell = Arc::clone(&cell);
            std::thread::spawn(move || cell.wait())
        };
        std::thread::sleep(Duration::from_millis(20));
        cell.set(Some("/p".into()));
        assert_eq!(waiter.join().ok().flatten().as_deref(), Some("/p"));
    }

    #[test]
    fn gives_none_at_the_deadline_and_ignores_a_late_path() {
        let cell = LoginPath::new();
        cell.expect(Duration::from_millis(20));
        assert_eq!(cell.wait(), None);
        cell.set(Some("/late".into()));
        assert_eq!(cell.wait(), None);
    }

    #[test]
    fn none_from_the_reader_ends_the_wait() {
        let cell = LoginPath::new();
        cell.expect(LONG);
        cell.set(None);
        assert_eq!(cell.wait(), None);
    }

    #[test]
    fn apply_sets_the_path_of_a_command() {
        let cell = LoginPath::new();
        cell.expect(LONG);
        cell.set(Some("/opt/bin".into()));
        let mut command = Command::new("git");
        apply_from(&cell, &mut command);
        let path = command
            .get_envs()
            .find(|(name, _)| *name == "PATH")
            .and_then(|(_, value)| value.map(|v| v.to_os_string()));
        assert_eq!(path.as_deref(), Some(std::ffi::OsStr::new("/opt/bin")));
        let mut plain = Command::new("git");
        apply_from(&LoginPath::new(), &mut plain);
        assert_eq!(plain.get_envs().count(), 0);
    }
}
