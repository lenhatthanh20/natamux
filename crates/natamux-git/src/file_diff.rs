//! Ports `packages/daemon/src/git/file-diff.ts`: both sides of a changed file, and the bytes of
//! a changed image, and the one way into a file of a worktree that refuses links out of it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use natamux_proto::workspaces::changes::{ChangeStatus, FileDiff, ImageSide, image_type_of};
use natamux_proto::workspaces::review_comments::CommentSide;
use natamux_proto::workspaces::workspace::ChangeEntry;

use crate::exec::{BlobRead, GitError, OutsideWorkspaceError, git, git_blob_limited, git_bytes};

/// A file bigger than this is not sent to the dashboard (`MAX_FILE_BYTES`).
pub use natamux_proto::workspaces::changes::{MAX_FILE_BYTES, MAX_IMAGE_BYTES};

/// The text of a file is binary when a zero byte shows up in its first bytes.
const BINARY_SCAN_BYTES: usize = 8000;

pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SCAN_BYTES).any(|byte| *byte == 0)
}

/// A file of the worktree, found through its real path.
#[derive(Clone, Debug)]
pub struct FileInWorktree {
    pub real: PathBuf,
    pub is_file: bool,
    pub is_folder: bool,
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

impl FileInWorktree {
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.real)
    }
}

/// The file in the worktree, or None when it is not there. A link that leaves the worktree is
/// refused. Node throws the raw error for a failure that is not "not found" (a file in the middle
/// of the path, no permission); here it reads as "not there", as the signature has no error for it.
pub fn read_inside(
    root: &Path,
    path: &str,
) -> Result<Option<FileInWorktree>, OutsideWorkspaceError> {
    let Ok(real) = root.join(path).canonicalize() else {
        return Ok(None);
    };
    let Ok(real_root) = root.canonicalize() else {
        return Ok(None);
    };
    if !real.starts_with(&real_root) {
        return Err(OutsideWorkspaceError(path.to_string()));
    }
    let Ok(stats) = std::fs::metadata(&real) else {
        return Ok(None);
    };
    Ok(Some(FileInWorktree {
        real,
        is_file: stats.is_file(),
        is_folder: stats.is_dir(),
        size: stats.len(),
        mtime: stats.modified().ok(),
    }))
}

/// Why a file or image could not be read. The daemon answers 409 for `Git` and `Outside`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDiffError {
    Git(GitError),
    Outside(OutsideWorkspaceError),
    /// The file is there but could not be read: a folder, or no permission.
    Read(String),
}

impl std::fmt::Display for FileDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(error) => write!(f, "{error}"),
            Self::Outside(error) => write!(f, "{error}"),
            Self::Read(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for FileDiffError {}

impl From<GitError> for FileDiffError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

impl From<OutsideWorkspaceError> for FileDiffError {
    fn from(error: OutsideWorkspaceError) -> Self {
        Self::Outside(error)
    }
}

fn read_file(file: &FileInWorktree) -> Result<Vec<u8>, FileDiffError> {
    file.read()
        .map_err(|error| FileDiffError::Read(error.to_string()))
}

// `Number(output.trim())`: a size that is not a number is never "too large".
fn object_size(worktree: &Path, source: &str) -> Result<u64, GitError> {
    let output = git(worktree, &["cat-file", "-s", source], None, &[])?;
    Ok(natamux_core::js::js_trim(&output).parse().unwrap_or(0))
}

/// Both sides of one changed file: the last commit, and the file as it is now.
pub fn read_file_diff(worktree: &Path, change: &ChangeEntry) -> Result<FileDiff, FileDiffError> {
    let diff = |original: Option<String>, modified: Option<String>, binary, too_large| FileDiff {
        path: change.path.clone(),
        status: change.status,
        original,
        modified,
        binary,
        too_large,
    };

    let mut original: Option<Vec<u8>> = None;
    if change.status != ChangeStatus::Added && change.status != ChangeStatus::Untracked {
        let from = change.original_path.as_ref().unwrap_or(&change.path);
        let source = format!("HEAD:{from}");
        match git_blob_limited(worktree, &source, MAX_FILE_BYTES)? {
            BlobRead::TooLarge => return Ok(diff(None, None, false, true)),
            BlobRead::Content(bytes) => original = Some(bytes),
        }
    }

    let mut modified: Option<Vec<u8>> = None;
    if change.status != ChangeStatus::Deleted
        && let Some(file) = read_inside(worktree, &change.path)?
    {
        if file.size > MAX_FILE_BYTES {
            return Ok(diff(None, None, false, true));
        }
        modified = Some(read_file(&file)?);
    }

    let binary = |side: &Option<Vec<u8>>| side.as_deref().is_some_and(looks_binary);
    if binary(&original) || binary(&modified) {
        return Ok(diff(None, None, true, false));
    }
    let text =
        |side: Option<Vec<u8>>| side.map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    Ok(diff(text(original), text(modified), false, false))
}

/// The text of one side of a file, for a review comment: the file now (`New`, also when it is not
/// in the change list) or the last commit (`Old`, from `original_path` for a rename). None when
/// the side is not there, is binary or is too large.
pub fn read_side(
    worktree: &Path,
    path: &str,
    original_path: Option<&str>,
    side: CommentSide,
) -> Result<Option<String>, FileDiffError> {
    let bytes = match side {
        CommentSide::New => {
            let Some(file) = read_inside(worktree, path)? else {
                return Ok(None);
            };
            if !file.is_file || file.size > MAX_FILE_BYTES {
                return Ok(None);
            }
            read_file(&file)?
        }
        CommentSide::Old => {
            let source = format!("HEAD:{}", original_path.unwrap_or(path));
            // A path that is not in the last commit has no old side; git fails for it.
            if git(worktree, &["cat-file", "-e", &source], None, &[]).is_err() {
                return Ok(None);
            }
            if object_size(worktree, &source)? > MAX_FILE_BYTES {
                return Ok(None);
            }
            git_bytes(worktree, &["show", &source])?
        }
    };
    if looks_binary(&bytes) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Why one side of an image could not be read. The daemon answers 400 for `NotAnImage`, 404 for
/// `SideMissing`, 413 for `TooLarge`, and 409 for `Git` and `Outside`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileImageError {
    NotAnImage(String),
    SideMissing(String, ImageSide),
    TooLarge(String),
    Git(GitError),
    Outside(OutsideWorkspaceError),
    Read(String),
}

impl std::fmt::Display for FileImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnImage(path) => {
                write!(f, "{path} is not an image that the dashboard can show")
            }
            Self::SideMissing(path, ImageSide::Original) => {
                write!(f, "{path} is not in the last commit")
            }
            Self::SideMissing(path, ImageSide::Modified) => {
                write!(f, "{path} is not in the workspace any more")
            }
            Self::TooLarge(path) => write!(
                f,
                "{path} is larger than {} MB, so it is not shown",
                MAX_IMAGE_BYTES / 1_000_000
            ),
            Self::Git(error) => write!(f, "{error}"),
            Self::Outside(error) => write!(f, "{error}"),
            Self::Read(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for FileImageError {}

impl From<GitError> for FileImageError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

impl From<OutsideWorkspaceError> for FileImageError {
    fn from(error: OutsideWorkspaceError) -> Self {
        Self::Outside(error)
    }
}

/// One side of a changed image: the last commit (`Original`) or the file now (`Modified`). Gives
/// the MIME type and the bytes.
pub fn read_file_image(
    worktree: &Path,
    change: &ChangeEntry,
    side: ImageSide,
) -> Result<(String, Vec<u8>), FileImageError> {
    let path = &change.path;
    let Some(image_type) = image_type_of(path) else {
        return Err(FileImageError::NotAnImage(path.clone()));
    };
    let missing = || FileImageError::SideMissing(path.clone(), side);

    if side == ImageSide::Original {
        let source = change.original_path.as_ref().unwrap_or(path);
        // A file renamed from something that is not an image has no old image to show.
        let original_type = image_type_of(source);
        let new_file =
            change.status == ChangeStatus::Added || change.status == ChangeStatus::Untracked;
        let Some(original_type) = original_type.filter(|_| !new_file) else {
            return Err(missing());
        };
        let from = format!("HEAD:{source}");
        if object_size(worktree, &from)? > MAX_IMAGE_BYTES {
            return Err(FileImageError::TooLarge(path.clone()));
        }
        return Ok((
            original_type.to_string(),
            git_bytes(worktree, &["show", &from])?,
        ));
    }

    if change.status == ChangeStatus::Deleted {
        return Err(missing());
    }
    let Some(file) = read_inside(worktree, path)? else {
        return Err(missing());
    };
    if file.size > MAX_IMAGE_BYTES {
        return Err(FileImageError::TooLarge(path.clone()));
    }
    let bytes = file
        .read()
        .map_err(|error| FileImageError::Read(error.to_string()))?;
    Ok((image_type.to_string(), bytes))
}

#[cfg(test)]
#[path = "file_diff_tests.rs"]
mod tests;
