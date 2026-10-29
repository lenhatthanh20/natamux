//! `workspace-files.ts`: the files explorer. Every request goes through `resolve_inside`: inside
//! the worktree (links too), not in `.git`, and there.

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

use natamux_core::workspaces::safe_write::write_inside;
use natamux_git::changes::locale_compare;
use natamux_git::exec::OutsideWorkspaceError;
use natamux_git::file_diff::{FileInWorktree, MAX_FILE_BYTES, looks_binary, read_inside};
use natamux_proto::workspaces::files::{EntryKind, SavedFile, TreeEntry, WorkspaceFile};
use sha2::{Digest, Sha256};

use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};

/// `versionOf`: the SHA-256 of the bytes, in hex.
pub fn version_of(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Any case: macOS and Windows find `.GIT` too.
fn is_in_git_folder(real_root: &Path, real: &Path) -> bool {
    real.strip_prefix(real_root).is_ok_and(|inside| {
        inside.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".git")
        })
    })
}

// Text that is not UTF-8 would come back changed after a save, so it is not opened as text.
fn text_of(bytes: Vec<u8>) -> Option<String> {
    if looks_binary(&bytes) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

// `path.resolve`: joins and drops `.` and `..` without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The path in the worktree, with the guards of every explorer request.
fn resolve_inside(root: &Path, path: &str) -> WorkspaceResult<FileInWorktree> {
    // Checked before the disk is read, so the answer never tells whether a file outside is there.
    let base = normalize(root);
    let wanted = normalize(&root.join(path));
    let Ok(inside) = wanted.strip_prefix(&base) else {
        return Err(OutsideWorkspaceError(path.to_string()).into());
    };
    // The cleaned path, so `src/../.git` finds `.git` even when `src` is not there.
    let found = read_inside(root, &inside.to_string_lossy())?
        .ok_or_else(|| WorkspaceError::PathNotFound(path.to_string()))?;
    let real_root = root
        .canonicalize()
        .map_err(|error| WorkspaceError::Io(error.to_string()))?;
    if is_in_git_folder(&real_root, &found.real) {
        return Err(WorkspaceError::GitFolder(path.to_string()));
    }
    Ok(found)
}

// The kind a link shows as; None for one that leaves the worktree or points nowhere.
fn linked_kind(real_root: &Path, link: &Path) -> Option<EntryKind> {
    let real = link.canonicalize().ok()?;
    if real == real_root || !real.starts_with(real_root) || is_in_git_folder(real_root, &real) {
        return None;
    }
    let stats = std::fs::metadata(&real).ok()?;
    if stats.is_dir() {
        Some(EntryKind::Folder)
    } else {
        stats.is_file().then_some(EntryKind::File)
    }
}

fn read_error(error: &std::io::Error) -> WorkspaceError {
    WorkspaceError::Io(error.to_string())
}

/// One folder of the worktree, folders first. A link shows as what it points to; one that leaves
/// the worktree or points nowhere is left out.
pub fn list_folder(root: &Path, path: &str) -> WorkspaceResult<Vec<TreeEntry>> {
    let folder = resolve_inside(root, path)?;
    if !folder.is_folder {
        return Err(WorkspaceError::NotA {
            path: path.to_string(),
            kind: "folder",
        });
    }
    let real_root = root.canonicalize().map_err(|error| read_error(&error))?;
    let mut entries = Vec::new();
    for listed in std::fs::read_dir(&folder.real).map_err(|error| read_error(&error))? {
        let listed = listed.map_err(|error| read_error(&error))?;
        let name = listed.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case(".git") {
            continue;
        }
        let Ok(kind) = listed.file_type() else {
            continue;
        };
        let kind = if kind.is_dir() {
            Some(EntryKind::Folder)
        } else if kind.is_file() {
            Some(EntryKind::File)
        } else if kind.is_symlink() {
            linked_kind(&real_root, &folder.real.join(&name))
        } else {
            None
        };
        if let Some(kind) = kind {
            entries.push(TreeEntry { name, kind });
        }
    }
    entries.sort_by(|a, b| match (a.kind, b.kind) {
        (EntryKind::Folder, EntryKind::File) => Ordering::Less,
        (EntryKind::File, EntryKind::Folder) => Ordering::Greater,
        _ => natural_compare(&a.name, &b.name),
    });
    Ok(entries)
}

/// The file with its text, or without it when it is binary, not UTF-8 or too large.
pub fn read_workspace_file(root: &Path, path: &str) -> WorkspaceResult<WorkspaceFile> {
    let file = resolve_inside(root, path)?;
    if !file.is_file {
        return Err(WorkspaceError::NotA {
            path: path.to_string(),
            kind: "file",
        });
    }
    let answer = |text: Option<String>, binary, too_large, version: String| WorkspaceFile {
        path: path.to_string(),
        text,
        binary,
        too_large,
        version,
    };
    if file.size > MAX_FILE_BYTES {
        return Ok(answer(None, false, true, String::new()));
    }
    let bytes = file.read().map_err(|error| read_error(&error))?;
    let version = version_of(&bytes);
    Ok(match text_of(bytes) {
        Some(text) => answer(Some(text), false, false, version),
        None => answer(None, true, false, version),
    })
}

/// Only a file that is there and still has the bytes the editor read. Written to a new file and
/// renamed over it (the mode is copied), so a link or a hard link is never written through.
pub fn write_workspace_file(
    root: &Path,
    path: &str,
    text: &str,
    version: &str,
) -> WorkspaceResult<SavedFile> {
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(WorkspaceError::TooLarge(path.to_string()));
    }
    let file = resolve_inside(root, path)?;
    if !file.is_file {
        return Err(WorkspaceError::NotA {
            path: path.to_string(),
            kind: "file",
        });
    }
    let bytes = file.read().map_err(|error| read_error(&error))?;
    if version_of(&bytes) != version {
        return Err(WorkspaceError::ChangedOnDisk(path.to_string()));
    }
    // The real path: a link inside the worktree is saved at the file it points to.
    let real_root = root.canonicalize().map_err(|error| read_error(&error))?;
    let inside = file
        .real
        .strip_prefix(&real_root)
        .map_err(|_| OutsideWorkspaceError(path.to_string()))?;
    write_inside(&real_root, inside, text.as_bytes()).map_err(|error| read_error(&error))?;
    Ok(SavedFile {
        path: path.to_string(),
        version: version_of(text.as_bytes()),
    })
}

enum Chunk<'a> {
    Digits(&'a str),
    Text(&'a str),
}

fn chunks(name: &str) -> Vec<Chunk<'_>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut digits = None;
    for (at, c) in name.char_indices() {
        let is_digit = c.is_ascii_digit();
        if digits.is_some_and(|before| before != is_digit) {
            parts.push(chunk(&name[start..at], digits == Some(true)));
            start = at;
        }
        digits = Some(is_digit);
    }
    if digits.is_some() {
        parts.push(chunk(&name[start..], digits == Some(true)));
    }
    parts
}

fn chunk(text: &str, digits: bool) -> Chunk<'_> {
    if digits {
        Chunk::Digits(text)
    } else {
        Chunk::Text(text)
    }
}

fn digits_compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// `localeCompare(b, undefined, { numeric: true })`: runs of digits compare as numbers, the rest
/// by `locale_compare`. A tie (such as `a1` and `a01`) falls back to the plain compare. shortcut:
/// no ICU, so accents and other scripts compare by code point.
pub fn natural_compare(a: &str, b: &str) -> Ordering {
    let (left, right) = (chunks(a), chunks(b));
    for (x, y) in left.iter().zip(&right) {
        let order = match (x, y) {
            (Chunk::Digits(x), Chunk::Digits(y)) => digits_compare(x, y),
            (Chunk::Text(x) | Chunk::Digits(x), Chunk::Text(y) | Chunk::Digits(y)) => {
                locale_compare(x, y)
            }
        };
        if order.is_ne() {
            return order;
        }
    }
    left.len()
        .cmp(&right.len())
        .then_with(|| locale_compare(a, b))
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
