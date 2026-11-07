//! The last screen of each open shell, in a folder beside the open-shells file. The daemon ends
//! with the app, so after a restart a shell that comes back shows its old screen at once, before
//! its new shell or agent has printed anything. Only shells listed in the open-shells file come
//! back, so a stale file of a closed shell is never shown.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use natamux_proto::terminals::query::is_terminal_name;

use super::shell_files::{read_open_shells, tasks_dir};

/// A saved screen holds at most `CATCH_UP_HISTORY_ROWS` of history, well under this. A bigger
/// file is not ours: it is not read.
const MAX_SCREEN_BYTES: u64 = 4 * 1024 * 1024;

/// Written after an old screen: back to the normal screen, default modes and pen, then a dim
/// line. The new shell starts below it, and no mode of the old program stays on.
pub const RESTORED_LINE: &str = "\x1b[?1049l\x1b[0m\x1b[?25h\x1b[?1l\x1b[?2004l\x1b[?1000l\
\x1b[?1002l\x1b[?1003l\x1b[?1006l\r\n\x1b[2m── Restored after restart ──\x1b[0m\r\n";

pub fn screens_dir(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{task_id}.screens"))
}

fn screen_path(data_dir: &Path, task_id: &str, terminal: &str) -> Option<PathBuf> {
    is_terminal_name(terminal)
        .then(|| screens_dir(data_dir, task_id).join(format!("{terminal}.ans")))
}

/// Written whole to a temporary file first, so a crash never leaves half a screen.
pub fn save_screen(
    data_dir: &Path,
    task_id: &str,
    terminal: &str,
    screen: &[u8],
) -> io::Result<()> {
    let Some(path) = screen_path(data_dir, task_id, terminal) else {
        return Ok(());
    };
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = path.with_extension("ans.tmp");
    fs::write(&temporary, screen)?;
    fs::rename(&temporary, path)
}

/// The old screen of a shell that comes back: None when the open-shells file does not list it,
/// or there is no usable file.
pub fn read_screen(data_dir: &Path, task_id: &str, terminal: &str) -> Option<Vec<u8>> {
    let path = screen_path(data_dir, task_id, terminal)?;
    let size = fs::metadata(&path).ok()?.len();
    if size == 0 || size > MAX_SCREEN_BYTES {
        return None;
    }
    let listed = read_open_shells(data_dir, task_id)
        .iter()
        .any(|shell| shell.name == terminal);
    if !listed {
        return None;
    }
    fs::read(path).ok()
}

/// Keeps the screens of the shells named, removes the others. No names: the whole folder goes.
pub fn keep_screens(data_dir: &Path, task_id: &str, names: &[&str]) -> io::Result<()> {
    let folder = screens_dir(data_dir, task_id);
    if names.is_empty() {
        return match fs::remove_dir_all(&folder) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let Ok(entries) = fs::read_dir(&folder) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let kept = name
            .to_str()
            .and_then(|name| name.strip_suffix(".ans"))
            .is_some_and(|terminal| names.contains(&terminal));
        if !kept {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminals::session_lifecycle::random_id;
    use crate::terminals::shell_files::{OpenShell, save_open_shells};

    struct TempDir(PathBuf);

    impl TempDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir() -> TempDir {
        let path = std::env::temp_dir().join(format!("natamux-screens-{}", random_id(8)));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn listed(data_dir: &Path, names: &[&str]) {
        let shells: Vec<OpenShell> = names
            .iter()
            .map(|name| OpenShell {
                name: name.to_string(),
                agent: None,
                session: None,
                paused: false,
            })
            .collect();
        save_open_shells(data_dir, "T-0001", &shells).unwrap();
    }

    #[test]
    fn a_listed_shell_gets_its_saved_screen_back() {
        let dir = temp_dir();
        listed(dir.path(), &["1"]);
        save_screen(dir.path(), "T-0001", "1", b"old screen").unwrap();
        assert_eq!(
            read_screen(dir.path(), "T-0001", "1").as_deref(),
            Some(&b"old screen"[..])
        );
    }

    #[test]
    fn a_shell_the_open_shells_file_does_not_list_gets_nothing() {
        let dir = temp_dir();
        listed(dir.path(), &["1"]);
        save_screen(dir.path(), "T-0001", "2", b"closed shell").unwrap();
        assert_eq!(read_screen(dir.path(), "T-0001", "2"), None);
    }

    #[test]
    fn a_name_that_is_not_a_terminal_name_is_never_a_path() {
        let dir = temp_dir();
        save_screen(dir.path(), "T-0001", "../x", b"no").unwrap();
        assert!(!screens_dir(dir.path(), "T-0001").exists());
        assert_eq!(read_screen(dir.path(), "T-0001", "../x"), None);
    }

    #[test]
    fn keep_removes_the_screens_of_shells_not_named() {
        let dir = temp_dir();
        for name in ["1", "2", "3"] {
            save_screen(dir.path(), "T-0001", name, b"x").unwrap();
        }
        keep_screens(dir.path(), "T-0001", &["1", "3"]).unwrap();
        let folder = screens_dir(dir.path(), "T-0001");
        assert!(folder.join("1.ans").exists());
        assert!(!folder.join("2.ans").exists());
        assert!(folder.join("3.ans").exists());
        keep_screens(dir.path(), "T-0001", &[]).unwrap();
        assert!(!folder.exists());
    }
}
