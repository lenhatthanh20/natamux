//! Ports `packages/daemon/src/stages/stage-files.ts`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use natamux_core::js::iso_string;
use natamux_core::workspaces::safe_write::{copy_inside, write_inside};
use natamux_store::task_files::{STAGE_DIR, archive_dir};

use crate::git::common_dir;

pub const MAX_STAGE_FILE_BYTES: usize = 1_000_000;

/// A stage file name or size the routes refuse; the text is the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageFileError(pub String);

impl std::fmt::Display for StageFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StageFileError {}

fn io_error(error: io::Error) -> StageFileError {
    StageFileError(error.to_string())
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// `FILE_NAME`: `^[\w-]+(\.[\w-]+)*\.md$`.
fn is_file_name(name: &str) -> bool {
    name.strip_suffix(".md").is_some_and(|rest| {
        rest.split('.')
            .all(|part| !part.is_empty() && part.chars().all(is_word))
    })
}

pub fn check_file_name(name: &str) -> Result<&str, StageFileError> {
    if is_file_name(name) {
        Ok(name)
    } else {
        Err(StageFileError(format!(
            "\"{name}\" is not a stage file. Stage files are markdown, like spec.md"
        )))
    }
}

/// Where a stage file is read and written now: the worktree while there is one.
pub fn working_dir(data_dir: &Path, task_id: &str, workspace: Option<&str>) -> PathBuf {
    match workspace {
        None => archive_dir(data_dir, task_id),
        Some(workspace) => Path::new(workspace).join(STAGE_DIR),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageFile {
    pub name: String,
    pub size: u64,
    /// ISO time, as `toISOString`.
    pub modified_at: String,
}

fn stage_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| is_file_name(name))
        .collect()
}

pub fn list_stage_files(data_dir: &Path, task_id: &str, workspace: Option<&str>) -> Vec<StageFile> {
    let dir = working_dir(data_dir, task_id, workspace);
    let mut names = stage_names(&dir);
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let info = fs::metadata(dir.join(&name)).ok()?;
            Some(StageFile {
                size: info.len(),
                modified_at: iso_string(info.modified().ok()?),
                name,
            })
        })
        .collect()
}

/// The text of a file, None when it is not there.
pub fn read_stage_file(
    data_dir: &Path,
    task_id: &str,
    workspace: Option<&str>,
    name: &str,
) -> Result<Option<String>, StageFileError> {
    let path = working_dir(data_dir, task_id, workspace).join(check_file_name(name)?);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(io_error)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

pub fn write_stage_file(
    data_dir: &Path,
    task_id: &str,
    workspace: Option<&str>,
    name: &str,
    text: &str,
) -> Result<(), StageFileError> {
    if text.len() > MAX_STAGE_FILE_BYTES {
        return Err(StageFileError(format!("{name} is larger than 1 MB")));
    }
    check_file_name(name)?;
    // The archive folder is ours (the data folder); only a worktree can hold a link of a repo.
    let relative = match workspace {
        Some(_) => Path::new(STAGE_DIR).join(name),
        None => Path::new(name).to_path_buf(),
    };
    let root = match workspace {
        Some(workspace) => PathBuf::from(workspace),
        None => {
            let archive = archive_dir(data_dir, task_id);
            fs::create_dir_all(&archive).map_err(io_error)?;
            archive
        }
    };
    write_inside(&root, &relative, text.as_bytes()).map_err(io_error)
}

/// Copies the stage files of one folder into `relative` below `root`; nothing when `from` is not
/// there. A link in `root` or `relative` is refused.
pub fn copy_stage_files(from: &Path, root: &Path, relative: &Path) -> io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    for name in stage_names(from) {
        copy_inside(root, &relative.join(&name), &from.join(&name))?;
    }
    Ok(())
}

/// Keeps a copy of the worktree's stage files with the task, so a removed workspace loses none.
pub fn archive_stage_files(
    data_dir: &Path,
    task_id: &str,
    workspace: Option<&str>,
) -> io::Result<()> {
    match workspace {
        // The archive folder is ours, so its root is the folder itself.
        Some(workspace) => {
            let archive = archive_dir(data_dir, task_id);
            if Path::new(workspace).join(STAGE_DIR).exists() {
                fs::create_dir_all(&archive)?;
            }
            copy_stage_files(
                &Path::new(workspace).join(STAGE_DIR),
                &archive,
                Path::new(""),
            )
        }
        None => Ok(()),
    }
}

/// Puts the archived stage files into a new worktree, so its agent finds them.
pub fn restore_stage_files(
    data_dir: &Path,
    task_id: &str,
    workspace: Option<&str>,
) -> io::Result<()> {
    match workspace {
        Some(workspace) => copy_stage_files(
            &archive_dir(data_dir, task_id),
            Path::new(workspace),
            Path::new(STAGE_DIR),
        ),
        None => Ok(()),
    }
}

/// Adds the stage folder to the repository's own ignore list (shared by all its worktrees), so
/// the files never reach a commit.
pub fn exclude_stage_dir(worktree: &Path) -> Result<(), String> {
    let common = match natamux_agents::git_files::common_dir(worktree) {
        Some(common) => common,
        None => worktree.join(common_dir(worktree).map_err(|error| error.to_string())?),
    };
    let info = common.join("info");
    let exclude = info.join("exclude");
    let line = format!("/{STAGE_DIR}/");
    let text = fs::read(&exclude)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    if text
        .split('\n')
        .any(|each| each.strip_suffix('\r').unwrap_or(each) == line)
    {
        return Ok(());
    }
    fs::create_dir_all(&info).map_err(|error| error.to_string())?;
    let lead = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&exclude)
        .map_err(|error| error.to_string())?;
    file.write_all(format!("{lead}{line}\n").as_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
