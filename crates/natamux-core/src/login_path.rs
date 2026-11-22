//! The PATH of the user's login shell, which the desktop app sends after the daemon started. The
//! daemon serves at once; only a program that is spawned waits for it. `set_var` is never used:
//! threads already run, so it is not safe.

use std::path::Path;
use std::process::Command;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::js::{is_js_space, js_trim};

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

/// Whether the program of a command line (its first word, `"…"` quotes taken off) is installed:
/// found on the login PATH, or the daemon's own PATH before the login one came. Starts no process.
/// shortcut: an alias or a function of the user's shell is not found, so that program counts as
/// not installed; asking the shell instead would start a login shell for each check.
pub fn is_on_path(program: &str) -> bool {
    let path = wait()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD;.PS1".into());
    find_program(first_word(program), &path, &pathext, cfg!(windows))
}

fn first_word(line: &str) -> &str {
    let line = js_trim(line);
    match line.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => line.split(is_js_space).next().unwrap_or(""),
    }
}

/// Whether `word` names a file: as a path when it has a separator, else in a folder of `path`.
/// On Windows a name without an extension also tries each ending of `pathext` (`.EXE;.CMD`).
pub fn find_program(word: &str, path: &str, pathext: &str, windows: bool) -> bool {
    if word.is_empty() {
        return false;
    }
    if word.contains('/') || (windows && word.contains('\\')) {
        return is_program_file(Path::new(word), pathext, windows);
    }
    let separator = if windows { ';' } else { ':' };
    path.split(separator)
        .filter(|folder| !folder.is_empty())
        .any(|folder| is_program_file(&Path::new(folder).join(word), pathext, windows))
}

fn is_program_file(file: &Path, pathext: &str, windows: bool) -> bool {
    if file.is_file() {
        return true;
    }
    windows
        && file.extension().is_none()
        && pathext
            .split(';')
            .map(|ending| ending.trim_start_matches('.'))
            .filter(|ending| !ending.is_empty())
            .any(|ending| file.with_extension(ending).is_file())
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

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("natamux-path-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // Unix paths: `:` splits the PATH and `/` marks a path.
    #[cfg(unix)]
    #[test]
    fn finds_a_program_in_a_later_folder_of_the_path() {
        let (empty, bin) = (folder("empty"), folder("bin"));
        std::fs::write(bin.join("codex"), "").unwrap();
        let path = format!("{}::{}", empty.display(), bin.display());
        assert!(find_program("codex", &path, "", false));
        assert!(!find_program("claude", &path, "", false));
        assert!(!find_program("", &path, "", false));
        // A folder of that name is not a program.
        std::fs::create_dir_all(bin.join("gemini")).unwrap();
        assert!(!find_program("gemini", &path, "", false));
    }

    #[cfg(unix)]
    #[test]
    fn checks_the_file_of_a_program_given_as_a_path() {
        let bin = folder("as-path");
        let file = bin.join("my-claude");
        std::fs::write(&file, "").unwrap();
        let word = file.to_string_lossy().into_owned();
        assert!(find_program(&word, "", "", false));
        assert!(!find_program(
            &format!("{word}-gone"),
            "/usr/bin",
            "",
            false
        ));
    }

    #[test]
    fn tries_each_ending_of_pathext_on_windows() {
        let bin = folder("pathext");
        std::fs::write(bin.join("claude.CMD"), "").unwrap();
        let path = bin.to_string_lossy().into_owned();
        assert!(find_program("claude", &path, ".EXE;.CMD", true));
        assert!(!find_program("claude", &path, ".EXE", true));
        assert!(!find_program("claude", &path, ".EXE;.CMD", false));
    }

    #[test]
    fn takes_the_first_word_of_a_program_line() {
        assert_eq!(first_word("  claude --model opus"), "claude");
        assert_eq!(
            first_word("\"/opt/my tools/codex\" exec"),
            "/opt/my tools/codex"
        );
        assert_eq!(first_word(""), "");
    }
}
