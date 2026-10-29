//! Writes into a worktree without following a link. A repo can hold a link where Natamux writes
//! (a stage file, a hook file); the write must never reach a file outside the worktree.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn refuse(path: &Path, why: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{}: {why}", path.display()),
    )
}

// The names below the root; a path that is absolute or leaves the root is refused.
fn parts_of(relative: &Path) -> io::Result<Vec<&std::ffi::OsStr>> {
    let mut parts = Vec::new();
    for part in relative.components() {
        match part {
            Component::Normal(name) => parts.push(name),
            Component::CurDir => {}
            _ => return Err(refuse(relative, "the path must stay inside the folder")),
        }
    }
    if parts.is_empty() {
        return Err(refuse(relative, "the path is empty"));
    }
    Ok(parts)
}

fn check_not_link(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(refuse(
            path,
            "is a link, so Natamux does not write through it",
        )),
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

// Each folder is made on its own, so no link can slip in between a check and a deep create.
fn enter_folder(path: &Path) -> io::Result<()> {
    if let Some(meta) = check_not_link(path)? {
        return if meta.is_dir() {
            Ok(())
        } else {
            Err(refuse(path, "is not a folder"))
        };
    }
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        // Someone made it first: it must be a real folder.
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => match check_not_link(path)? {
            Some(meta) if meta.is_dir() => Ok(()),
            _ => Err(refuse(path, "is not a folder")),
        },
        Err(error) => Err(error),
    }
}

fn write_new(path: &Path, like: Option<&fs::Metadata>, bytes: &[u8]) -> io::Result<()> {
    // `create_new` fails on any existing name, a dangling link too, and never follows one.
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = fill(&mut file, like, bytes);
    if written.is_err() {
        let _ = fs::remove_file(path);
    }
    written
}

fn fill(file: &mut fs::File, like: Option<&fs::Metadata>, bytes: &[u8]) -> io::Result<()> {
    if let Some(meta) = like.filter(|meta| meta.is_file()) {
        file.set_permissions(meta.permissions())?;
    }
    file.write_all(bytes)?;
    file.sync_all()
}

/// Writes `bytes` to `root/relative`. Refuses a link on the way or at the target. Missing folders
/// are made. A file that is there already is written next to its target and renamed over it, so a
/// hard link to a file outside is replaced, not written through.
pub fn write_inside(root: &Path, relative: &Path, bytes: &[u8]) -> io::Result<()> {
    let parts = parts_of(relative)?;
    let Some((name, folders)) = parts.split_last() else {
        return Err(refuse(relative, "the path is empty"));
    };
    let mut folder: PathBuf = root.to_path_buf();
    for part in folders {
        folder.push(part);
        enter_folder(&folder)?;
    }
    let target = folder.join(name);
    // A new file is made in place: a temp name would show in git status, as only the real name
    // is in the exclude file.
    let Some(target_meta) = check_not_link(&target)? else {
        return write_new(&target, None, bytes);
    };
    // Hook files are written again at each agent start; the same text needs no temp file either.
    if target_meta.is_file() && fs::read(&target).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(
        ".natamux-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let temp = folder.join(temp_name);
    write_new(&temp, Some(&target_meta), bytes)?;
    let renamed = fs::rename(&temp, &target);
    if renamed.is_err() {
        let _ = fs::remove_file(&temp);
    }
    renamed
}

/// Copies the file `from` to `root/relative` with the same guards as `write_inside`.
pub fn copy_inside(root: &Path, relative: &Path, from: &Path) -> io::Result<()> {
    write_inside(root, relative, &fs::read(from)?)
}

#[cfg(test)]
#[path = "safe_write_tests.rs"]
mod tests;
