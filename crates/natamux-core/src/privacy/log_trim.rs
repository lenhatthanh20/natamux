//! Keeps a log file under a size: the newest bytes stay, cut at a line start. Shared by the app
//! (app, daemon and update logs) and the daemon (audit log).

use std::fs;
use std::io;
use std::path::Path;

use crate::workspaces::private_dir::write_private;

/// Keeps the last `keep` bytes of `path`, from a line start. True when the file was cut. A
/// missing file does nothing; a link is left as it is, since it may point outside the data folder.
pub fn trim_file(path: &Path, keep: u64) -> io::Result<bool> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !meta.is_file() || meta.len() <= keep {
        return Ok(false);
    }
    let bytes = fs::read(path)?;
    let keep = usize::try_from(keep).unwrap_or(usize::MAX);
    let Some(start) = bytes.len().checked_sub(keep).filter(|start| *start > 0) else {
        return Ok(false);
    };
    write_private(path, bytes.get(cut_at(&bytes, start)..).unwrap_or_default())?;
    Ok(true)
}

/// The first line start at or after `start`.
fn cut_at(bytes: &[u8], start: usize) -> usize {
    if start == 0 || bytes.get(start - 1) == Some(&b'\n') {
        return start;
    }
    let rest = bytes.get(start..).unwrap_or_default();
    match rest.iter().position(|byte| *byte == b'\n') {
        Some(index) => start + index + 1,
        None => bytes.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("natamux-trim-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keeps_the_last_bytes_from_a_line_start() {
        let dir = folder("cut");
        let path = dir.join("a.log");
        fs::write(&path, "aaaa\nbbbb\ncccc\n").unwrap();
        // The cut falls inside "bbbb": that line goes whole.
        assert!(trim_file(&path, 8).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "cccc\n");
        // The cut falls on a line start: the line stays.
        fs::write(&path, "aaaa\nbbbb\ncccc\n").unwrap();
        assert!(trim_file(&path, 10).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "bbbb\ncccc\n");
        // A small file stays as it is.
        assert!(!trim_file(&path, 100).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "bbbb\ncccc\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_or_a_folder_does_nothing() {
        let dir = folder("none");
        assert!(!trim_file(&dir.join("none.log"), 1).unwrap());
        assert!(!trim_file(&dir, 1).unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }
}
