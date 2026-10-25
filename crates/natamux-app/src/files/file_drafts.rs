//! Unsaved edits of files of the worktree, and the rules of the editor tab around them
//! (`features/files/file-drafts.ts`, `FileEditor.tsx`).

use std::collections::BTreeMap;

/// An unsaved edit. `version` is the version of the file the edit began on; a save sends it, so a
/// file that changed since is not replaced unseen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDraft {
    pub text: String,
    pub version: String,
}

/// One draft per task and file. It lives while the app does.
#[derive(Clone, Debug, Default)]
pub struct Drafts {
    items: BTreeMap<(String, String), FileDraft>,
}

impl Drafts {
    pub fn get(&self, task: &str, path: &str) -> Option<&FileDraft> {
        self.items.get(&(task.to_string(), path.to_string()))
    }

    /// None drops the draft.
    pub fn set(&mut self, task: &str, path: &str, draft: Option<FileDraft>) {
        let key = (task.to_string(), path.to_string());
        match draft {
            Some(draft) => {
                self.items.insert(key, draft);
            }
            None => {
                self.items.remove(&key);
            }
        }
    }

    #[cfg(test)]
    pub fn has_any(&self) -> bool {
        !self.items.is_empty()
    }

    /// The files of one task that have a draft, with the drafts.
    #[cfg(test)]
    pub fn of_task<'a>(&'a self, task: &'a str) -> impl Iterator<Item = (&'a str, &'a FileDraft)> {
        self.items
            .iter()
            .filter(move |((owner, _), _)| owner == task)
            .map(|((_, path), draft)| (path.as_str(), draft))
    }

    /// Every draft as ((task, path), draft).
    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = (&(String, String), &FileDraft)> {
        self.items.iter()
    }

    /// (task, path) of every draft, for `quit_question`.
    pub fn unsaved(&self) -> Vec<(String, String)> {
        self.items.keys().cloned().collect()
    }
}

/// The draft after an edit by the user. The text and version on disk again: no draft.
pub fn edit_draft(
    disk_text: &str,
    disk_version: &str,
    draft: Option<&FileDraft>,
    text: &str,
) -> Option<FileDraft> {
    let version = draft.map_or(disk_version, |d| d.version.as_str());
    if text == disk_text && version == disk_version {
        None
    } else {
        Some(FileDraft {
            text: text.to_string(),
            version: version.to_string(),
        })
    }
}

/// The edit began on an older version: the agent or another program wrote the file since.
pub fn is_stale(draft: Option<&FileDraft>, disk_version: Option<&str>) -> bool {
    matches!((draft, disk_version), (Some(d), Some(v)) if d.version != v)
}

/// The draft after a save. Text typed while the save ran stays a draft, now on the saved version.
pub fn after_save(
    now: Option<&FileDraft>,
    saved_text: &str,
    new_version: &str,
) -> Option<FileDraft> {
    let now = now?;
    if now.text == saved_text {
        None
    } else {
        Some(FileDraft {
            text: now.text.clone(),
            version: new_version.to_string(),
        })
    }
}

/// "Keep mine": the draft takes the version on disk, so the next save replaces the file.
pub fn keep_mine(draft: &FileDraft, disk_version: &str) -> FileDraft {
    FileDraft {
        text: draft.text.clone(),
        version: disk_version.to_string(),
    }
}

/// Save needs a draft, no banner and no save in flight.
pub fn can_save(draft: Option<&FileDraft>, stale: bool, saving: bool) -> bool {
    draft.is_some() && !stale && !saving
}

/// The line shown in place of the editor, if any.
pub fn body_notice(binary: bool, too_large: bool, text_is_none: bool) -> Option<&'static str> {
    if binary {
        Some("Binary file. Not shown.")
    } else if too_large || text_is_none {
        Some("Larger than 1 MB. Not shown.")
    } else {
        None
    }
}

pub fn revert_question(name: &str) -> String {
    format!("Drop your changes to {name}?")
}

pub fn close_question(name: &str) -> String {
    format!("Discard your changes to {name}?")
}

/// The question before the app quits with unsaved edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuitQuestion {
    pub title: &'static str,
    pub rows: Vec<String>,
}

const QUIT_ROWS: usize = 5;

/// Each list is (task, path or file name). None when there is no draft.
pub fn quit_question(
    file_drafts: &[(String, String)],
    doc_drafts: &[(String, String)],
) -> Option<QuitQuestion> {
    let all: Vec<&(String, String)> = file_drafts.iter().chain(doc_drafts).collect();
    if all.is_empty() {
        return None;
    }
    let mut rows: Vec<String> = all
        .iter()
        .take(QUIT_ROWS)
        .map(|(task, path)| format!("{task} \u{b7} {path}"))
        .collect();
    if all.len() > QUIT_ROWS {
        rows.push(format!("and {} more", all.len() - QUIT_ROWS));
    }
    Some(QuitQuestion {
        title: "Quit and drop your changes?",
        rows,
    })
}

#[cfg(test)]
#[path = "file_drafts_tests.rs"]
mod tests;
