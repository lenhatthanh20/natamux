//! A folder only the user can enter. Natamux keeps tokens, logs and task data under its folders,
//! so a user on the same machine must not read them.

use std::io;
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

#[cfg(test)]
#[path = "private_dir_tests.rs"]
mod tests;
