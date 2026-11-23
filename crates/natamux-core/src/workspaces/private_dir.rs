//! A folder only the user can enter. Natamux keeps tokens, logs and task data under its folders,
//! so a user on the same machine must not read them.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

/// Creates `path` (and its parents) with mode 0700, or tightens it when it is wider. Unix only;
/// other systems keep the plain folder.
#[cfg(unix)]
pub fn private_dir(path: &Path) -> io::Result<()> {
    use std::fs::Permissions;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    // `recursive` leaves an old folder as it was, so the mode is checked here.
    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        std::fs::set_permissions(path, Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn private_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Makes `path` a file only the user can read: mode 0600 when it is wider. A link is left as it
/// is. Unix only.
#[cfg(unix)]
pub fn private_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::symlink_metadata(path)?;
    // A chmod would change the file a link points to, which may not be ours.
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn private_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Create and append, and a new file gets mode 0600 (Unix). An old file keeps its mode: the
/// daemon tightens those once at start.
pub fn append_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Writes the whole file with mode 0600, also when an old file was wider. A temporary file
/// written here keeps that mode through a rename.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    private_file(path)
}

/// Adds `line` and a line break to a log file, with secrets removed. The folder is made private.
pub fn append_log_line(path: &Path, line: &str) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        private_dir(folder)?;
    }
    let text = format!("{}\n", crate::privacy::redact::redact_text(line));
    append_options().open(path)?.write_all(text.as_bytes())
}

#[cfg(test)]
#[path = "private_dir_tests.rs"]
mod tests;
