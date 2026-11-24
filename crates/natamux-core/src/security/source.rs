//! `security.json` in the data folder, read again at most once per second, and the tamper check:
//! the daemon keeps a copy of the last bytes it wrote (`security-written.json`). When the file
//! differs from that copy, it changed outside `PUT /security`, so the stricter of the two counts
//! and a warning is shown until the next save. While the daemon runs, the copy it compares with
//! is the one in memory: changing both files on disk is still an outside change. At start it
//! trusts the copy on disk.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::decide::tighten;
use super::settings::{SecurityState, read_text};
use crate::workspaces::private_dir::{private_dir, write_private};

pub const FILE_NAME: &str = "security.json";
pub const WRITTEN_NAME: &str = "security-written.json";
pub const OUTSIDE_CHANGE_WARNING: &str = "security.json changed outside Natamux, so the stricter \
     of the old and the new settings is used. Review the Security settings and save them.";

/// How often a decision may read the files again.
const RECHECK: Duration = Duration::from_secs(1);

struct Inner {
    file: Option<Vec<u8>>,
    written: Option<Vec<u8>>,
    state: SecurityState,
    checked: Instant,
    /// Goes up each time the state changes, so a watcher can tell.
    generation: u64,
}

pub struct Source {
    path: PathBuf,
    written_path: PathBuf,
    inner: Mutex<Inner>,
}

pub(super) fn read(path: &Path) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        // A file that exists but cannot be read counts as broken: Strict with defaults.
        Err(_) => Some(Vec::new()),
    }
}

fn state_of(bytes: Option<&[u8]>) -> SecurityState {
    match bytes {
        None => SecurityState::default(),
        Some(bytes) => read_text(&String::from_utf8_lossy(bytes)),
    }
}

/// The copy of what the daemon last wrote in `data_dir`: the installed Source's copy in memory
/// when it is for this folder, so changing both files on disk is still an outside change. Else
/// (no Source installed, as in the CLI or the app, or another folder) the copy on disk.
pub(super) fn written_in(data_dir: &Path) -> Option<Vec<u8>> {
    match super::installed() {
        Some(source) if source.path.parent() == Some(data_dir) => source.written(),
        _ => read(&data_dir.join(WRITTEN_NAME)),
    }
}

/// The state of the file, given the copy of what the daemon last wrote.
pub fn evaluate(file: Option<&[u8]>, written: Option<&[u8]>) -> SecurityState {
    let now = state_of(file);
    if file == written {
        return now;
    }
    let before = state_of(written);
    SecurityState {
        mode: tighten(&before.mode, &now.mode),
        ignored: now.ignored,
        warning: Some(OUTSIDE_CHANGE_WARNING.to_string()),
    }
}

impl Source {
    /// The settings in `data_dir`. A missing file is Personal mode.
    pub fn open(data_dir: &Path) -> Source {
        let path = data_dir.join(FILE_NAME);
        let written_path = data_dir.join(WRITTEN_NAME);
        let (file, written) = (read(&path), read(&written_path));
        let state = evaluate(file.as_deref(), written.as_deref());
        Source {
            path,
            written_path,
            inner: Mutex::new(Inner {
                file,
                written,
                state,
                checked: Instant::now(),
                generation: 0,
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The bytes this Source last wrote, or read from the copy on disk at open.
    pub fn written(&self) -> Option<Vec<u8>> {
        self.lock().written.clone()
    }

    /// A panic that held the lock leaves the last state, which is still the right one: falling
    /// back to the default would turn Strict off.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The state now; reads the file again when the last read is a second old.
    pub fn current(&self) -> SecurityState {
        let mut inner = self.lock();
        if inner.checked.elapsed() >= RECHECK {
            self.reload(&mut inner);
        }
        inner.state.clone()
    }

    /// Reads the files now. Gives the generation, which goes up when the state changed.
    pub fn check_now(&self) -> u64 {
        let mut inner = self.lock();
        self.reload(&mut inner);
        inner.generation
    }

    pub fn generation(&self) -> u64 {
        self.lock().generation
    }

    /// The bytes are compared, not the file time: two edits in one second with the same size
    /// would look the same by time and size. The copy on disk is not read again: whoever can
    /// change the file can change the copy too.
    fn reload(&self, inner: &mut Inner) {
        inner.checked = Instant::now();
        let file = read(&self.path);
        if file == inner.file {
            return;
        }
        let mut state = evaluate(file.as_deref(), inner.written.as_deref());
        // An outside change that is undone later must not loosen what was kept meanwhile.
        if inner.state.warning.is_some() && file != inner.written {
            state.mode = tighten(&inner.state.mode, &state.mode);
        }
        inner.file = file;
        self.set(inner, state);
    }

    fn set(&self, inner: &mut Inner, state: SecurityState) {
        if state != inner.state {
            inner.state = state;
            inner.generation += 1;
        }
    }

    /// The save of `PUT /security`: the copy first, then the file, so a crash in between reads
    /// as an outside change and keeps the stricter state.
    pub fn write(&self, text: &str) -> io::Result<SecurityState> {
        let mut inner = self.lock();
        if let Some(folder) = self.path.parent() {
            private_dir(folder)?;
        }
        write_private(&self.written_path, text.as_bytes())?;
        write_private(&self.path, text.as_bytes())?;
        let bytes = Some(text.as_bytes().to_vec());
        inner.file = bytes.clone();
        inner.written = bytes;
        inner.checked = Instant::now();
        let state = evaluate(Some(text.as_bytes()), Some(text.as_bytes()));
        self.set(&mut inner, state.clone());
        Ok(state)
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
