//! The files attached to a task (`attachments.ts`). They are written once, when the task is made,
//! into `<data>/tasks/<ID>/attachments`, and stay after its workspace is removed. A worktree gets
//! a copy next to its stage files.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use natamux_core::tasks::attachments::{
    ATTACHMENTS_DIR, format_bytes, preview_type_of, safe_attachment_name,
};
use natamux_proto::tasks::attachments::Attachment;

use crate::task_files::{STAGE_DIR, archive_dir};

/// A file with its safe name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub fn attachments_dir(data_dir: &Path, task_id: &str) -> PathBuf {
    archive_dir(data_dir, task_id).join(ATTACHMENTS_DIR)
}

/// Where an agent finds an attachment, from the root of the task's worktree.
pub fn attachment_path(name: &str) -> String {
    format!("{STAGE_DIR}/{ATTACHMENTS_DIR}/{name}")
}

/// The names must come from `attachment_names` or from an earlier task, so they are safe.
pub fn save_attachments(
    data_dir: &Path,
    task_id: &str,
    files: &[AttachmentFile],
) -> io::Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let dir = attachments_dir(data_dir, task_id);
    fs::create_dir_all(&dir)?;
    for file in files {
        fs::write(dir.join(safe_attachment_name(&file.name)), &file.bytes)?;
    }
    Ok(())
}

pub fn list_attachments(data_dir: &Path, task_id: &str) -> io::Result<Vec<Attachment>> {
    let dir = attachments_dir(data_dir, task_id);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file() && safe_attachment_name(&name) == name {
            names.push(name);
        }
    }
    // Safe names are ASCII: byte order is the order of Array#sort.
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let size = fs::metadata(dir.join(&name))?.len();
            let kind = preview_type_of(&name).map(str::to_string);
            Ok(Attachment { name, size, kind })
        })
        .collect()
}

/// A name that is not the name of an attachment.
#[derive(Debug, PartialEq, Eq)]
pub struct UnsafeName(pub String);

impl std::fmt::Display for UnsafeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\" is not the name of an attachment", self.0)
    }
}

/// The bytes of one attachment; None when the task has none of that name.
pub fn read_attachment(
    data_dir: &Path,
    task_id: &str,
    name: &str,
) -> Result<io::Result<Option<Vec<u8>>>, UnsafeName> {
    if safe_attachment_name(name) != name {
        return Err(UnsafeName(name.to_string()));
    }
    let path = attachments_dir(data_dir, task_id).join(name);
    Ok(match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::IsADirectory) => {
            Ok(None)
        }
        Err(error) => Err(error),
    })
}

/// All attachments of a task, for a copy of the task.
pub fn read_attachments(data_dir: &Path, task_id: &str) -> io::Result<Vec<AttachmentFile>> {
    let dir = attachments_dir(data_dir, task_id);
    list_attachments(data_dir, task_id)?
        .into_iter()
        .map(|attachment| {
            let bytes = fs::read(dir.join(&attachment.name))?;
            Ok(AttachmentFile {
                name: attachment.name,
                bytes,
            })
        })
        .collect()
}

/// Puts the attachments into a new worktree, next to the stage files, so its agent finds them.
pub fn restore_attachments(
    data_dir: &Path,
    task_id: &str,
    workspace: Option<&Path>,
) -> io::Result<()> {
    let attachments = list_attachments(data_dir, task_id)?;
    let Some(workspace) = workspace else {
        return Ok(());
    };
    if attachments.is_empty() {
        return Ok(());
    }
    let from = attachments_dir(data_dir, task_id);
    let to = workspace.join(STAGE_DIR).join(ATTACHMENTS_DIR);
    fs::create_dir_all(&to)?;
    for attachment in attachments {
        fs::copy(from.join(&attachment.name), to.join(&attachment.name))?;
    }
    Ok(())
}

pub fn attachment_section(attachments: &[Attachment]) -> String {
    let mut lines = vec![
        "## Attachments".to_string(),
        String::new(),
        "Files attached to the task, by their path from the root of the worktree:".into(),
        String::new(),
    ];
    for attachment in attachments {
        lines.push(format!(
            "- `{}` ({})",
            attachment_path(&attachment.name),
            format_bytes(attachment.size)
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

/// `/^#{1,6}[ \t]+Attachments[ \t]*$/im`.
fn has_attachments_heading(text: &str) -> bool {
    text.split('\n').any(|line| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let rest = &line[hashes..];
        let spaced = rest.trim_start_matches([' ', '\t']);
        (1..=6).contains(&hashes)
            && spaced.len() < rest.len()
            && spaced
                .trim_end_matches([' ', '\t'])
                .eq_ignore_ascii_case("attachments")
    })
}

/// The context with an Attachments section at its end, unless it has one already.
pub fn with_attachment_section(text: &str, attachments: &[Attachment]) -> String {
    if attachments.is_empty() || has_attachments_heading(text) {
        return text.to_string();
    }
    let gap = if text.is_empty() || text.ends_with("\n\n") {
        ""
    } else if text.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{text}{gap}{}", attachment_section(attachments))
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
