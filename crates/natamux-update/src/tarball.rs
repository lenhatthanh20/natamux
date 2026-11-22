//! Update an unpacked `.tar.gz` copy: unpack the verified archive, then swap its files in.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const BINARIES: [&str; 4] = ["natamux-app", "natamux-daemon", "natamux", "nmx"];
const NEW_SUFFIX: &str = ".natamux-new";

/// An unpacked `.tar.gz` has these beside the app; a `cargo build` folder has no desktop file.
pub fn is_tarball_layout(folder: &Path) -> bool {
    folder.join("natamux-daemon").is_file() && folder.join("natamux.desktop").is_file()
}

/// Unpack into a fresh `dir` with the system `tar`, then check what came out.
pub fn unpack_tarball(archive: &Path, dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    fs::create_dir_all(dir)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(dir)
        .arg("--no-same-owner")
        .status()?;
    if !status.success() {
        return Err(io::Error::other("The update archive could not be unpacked"));
    }
    check_unpacked(dir)
}

/// Only top-level regular files, and every binary the app needs.
pub fn check_unpacked(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        // file_type does not follow links, so a link is refused here.
        if !entry.file_type()?.is_file() {
            return Err(io::Error::other(format!(
                "The update archive has an unexpected entry: {}",
                entry.file_name().to_string_lossy()
            )));
        }
    }
    for name in BINARIES {
        if !dir.join(name).is_file() {
            return Err(io::Error::other(format!(
                "The update archive has no {name}"
            )));
        }
    }
    Ok(())
}

/// A `<name>.natamux-new` left in `target` by an earlier try, for a file in `staged`.
pub fn stale_tarball(staged: &Path, target: &Path) -> Option<PathBuf> {
    names(staged)
        .ok()?
        .iter()
        .map(|name| new_path(target, name))
        .find(|path| path.exists())
}

/// Phase 1 copies every staged file to `<target>/<name>.natamux-new`; any error removes those
/// copies and leaves the old files alone. Phase 2 renames them over the old files. Linux lets a
/// running binary be renamed over, so this runs before the app quits.
pub fn replace_folder(staged: &Path, target: &Path) -> io::Result<()> {
    let names = names(staged)?;
    if let Some(path) = names
        .iter()
        .map(|name| new_path(target, name))
        .find(|path| path.exists())
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("An old update file is in the way: {}", path.display()),
        ));
    }
    let mut written = Vec::new();
    for name in &names {
        if let Err(error) = copy_new(&staged.join(name), target, name, &mut written) {
            for path in &written {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
    }
    for name in &names {
        fs::rename(new_path(target, name), target.join(name))?;
    }
    Ok(())
}

fn copy_new(
    source: &Path,
    target: &Path,
    name: &OsString,
    written: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let path = new_path(target, name);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    written.push(path.clone());
    io::copy(&mut fs::File::open(source)?, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let binary = BINARIES.iter().any(|binary| name.to_str() == Some(*binary));
        let mode = if binary { 0o755 } else { 0o644 };
        fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

fn names(dir: &Path) -> io::Result<Vec<OsString>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

fn new_path(target: &Path, name: &OsString) -> PathBuf {
    let mut name = name.clone();
    name.push(NEW_SUFFIX);
    target.join(name)
}
