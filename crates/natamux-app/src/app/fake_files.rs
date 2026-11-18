//! The fake's files of screen 6: the worktree of a task (its tree, file texts and versions), the
//! stage files with their texts, and the attachments, with the daemon's writes on them. A save
//! checks the version and a write sends `WorkspaceChanged`, as the daemon does. The data is
//! `fake_files.json`; the two changed files of screen 4 take their text from `fake_git.json`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::iter::Peekable;
use std::str::Chars;

use natamux_client::{ClientError, DashboardEvent};
use natamux_proto::stages::actions::{StageFile, StageFileText};
use natamux_proto::tasks::attachments::Attachment;
use natamux_proto::workspaces::changes::FileDiff;
use natamux_proto::workspaces::files::{EntryKind, SavedFile, TreeEntry, WorkspaceFile};
use serde_json::Value;

use super::{Answer, Data, FakeDaemon, NOW, not_found};

const SEED: &str = include_str!("fake_files.json");
const GIT_SEED: &str = include_str!("fake_git.json");
/// The bytes of the seeded `screen.png`: the banner of screen 4.
const BANNER: &[u8] = include_bytes!("../../assets/fake/banner.png");
const PDF: &[u8] = b"%PDF-1.4\n% a fake notes file\n";

struct FileState {
    text: Option<String>,
    too_large: bool,
    /// Moves on with each save, so the version does.
    generation: u32,
}

impl FileState {
    fn version(&self, path: &str) -> String {
        if self.too_large {
            String::new()
        } else {
            format!("v{}-{path}", self.generation)
        }
    }
}

#[derive(Default)]
pub(super) struct FilesState {
    trees: BTreeMap<String, BTreeMap<String, FileState>>,
    stage_texts: BTreeMap<String, BTreeMap<String, String>>,
    attachments: BTreeMap<String, Vec<Attachment>>,
}

fn status(code: u16, message: String) -> ClientError {
    ClientError::Status {
        status: code,
        message,
    }
}

fn digits(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        digits.push(digit);
    }
    digits.trim_start_matches('0').to_string()
}

/// The daemon sorts names with numbers in order: `a2` before `a10`.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (left, right) = (digits(&mut a), digits(&mut b));
                let order = left.len().cmp(&right.len()).then(left.cmp(&right));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

impl Data {
    /// Adds the worktrees, stage file texts and attachments of screen 6 to the seed. Runs after
    /// the screen-3 and screen-4 seeds: it fixes the sizes of the stage files to their texts.
    pub(super) fn add_files_seed(&mut self) {
        let seed: Value = serde_json::from_str(SEED).unwrap_or(Value::Null);
        // Each tree: `files` (path to text; null for a file with no text) and `tooLarge`.
        let trees: BTreeMap<String, Value> = part!(seed, "worktrees");
        let git: Value = serde_json::from_str(GIT_SEED).unwrap_or(Value::Null);
        let diffs: BTreeMap<String, Vec<FileDiff>> = part!(git, "files");
        for (task, tree) in trees {
            let mut files: BTreeMap<String, FileState> = BTreeMap::new();
            let tree_files: BTreeMap<String, Option<String>> = part!(tree, "files");
            let large: BTreeSet<String> = part!(tree, "tooLarge");
            for (path, text) in tree_files {
                let too_large = large.contains(&path);
                files.insert(
                    path,
                    FileState {
                        text,
                        too_large,
                        generation: 1,
                    },
                );
            }
            for diff in diffs.get(&task).into_iter().flatten() {
                if let Some(text) = &diff.modified {
                    files.insert(
                        diff.path.clone(),
                        FileState {
                            text: Some(text.clone()),
                            too_large: false,
                            generation: 1,
                        },
                    );
                }
            }
            self.files.trees.insert(task, files);
        }
        let texts: BTreeMap<String, BTreeMap<String, String>> = part!(seed, "stageTexts");
        for (task, files) in &texts {
            for (name, text) in files {
                if let Some(listed) = self.stage_file_mut(task, name) {
                    listed.size = text.len() as u64;
                }
                let tree = self.files.trees.entry(task.clone()).or_default();
                tree.insert(
                    format!(".natamux-task/{name}"),
                    FileState {
                        text: Some(text.clone()),
                        too_large: false,
                        generation: 1,
                    },
                );
            }
        }
        self.files.stage_texts = texts;
        let attachments: BTreeMap<String, Vec<Attachment>> = part!(seed, "attachments");
        self.files.attachments = attachments
            .into_iter()
            .map(|(task, list)| {
                let sized = list
                    .into_iter()
                    .map(|mut each| {
                        each.size = attachment_bytes(&each.name).len() as u64;
                        each
                    })
                    .collect();
                (task, sized)
            })
            .collect();
    }

    fn stage_file_mut(&mut self, task_id: &str, name: &str) -> Option<&mut StageFile> {
        let list = self.stage_files.get_mut(task_id)?;
        list.iter_mut().find(|file| file.name == name)
    }

    /// A task that has a worktree; the daemon's 404 and 409 otherwise.
    fn worktree(&self, task_id: &str) -> Answer<&BTreeMap<String, FileState>> {
        let task = self.tasks.iter().find(|task| task.id == task_id);
        let task = task.ok_or_else(|| not_found("Task", task_id))?;
        if task.workspace.is_none() {
            return Err(status(409, format!("{task_id} has no workspace")));
        }
        static EMPTY: BTreeMap<String, FileState> = BTreeMap::new();
        Ok(self.files.trees.get(task_id).unwrap_or(&EMPTY))
    }

    fn changed_workspace(&mut self, task_id: &str) {
        self.send(DashboardEvent::WorkspaceChanged {
            task_id: task_id.into(),
        });
    }
}

fn attachment_bytes(name: &str) -> &'static [u8] {
    if name.ends_with(".png") { BANNER } else { PDF }
}

/// The path the daemon guards every explorer request with: not outside the worktree.
fn check_inside(path: &str) -> Answer<()> {
    if path.split('/').any(|part| part == "..") || path.starts_with('/') {
        return Err(status(
            409,
            format!("{path} points outside the workspace, so it is not shown"),
        ));
    }
    Ok(())
}

fn not_in_workspace(path: &str) -> ClientError {
    status(404, format!("{path} is not in the workspace"))
}

impl FakeDaemon {
    /// One folder, folders first. The root is the empty path.
    pub fn list_folder(&self, task_id: &str, folder: &str) -> Answer<Vec<TreeEntry>> {
        check_inside(folder)?;
        let data = self.data();
        let tree = data.worktree(task_id)?;
        let prefix = if folder.is_empty() {
            String::new()
        } else {
            format!("{}/", folder.trim_end_matches('/'))
        };
        if tree.contains_key(folder) {
            return Err(status(400, format!("{folder} is not a folder")));
        }
        let mut entries: BTreeMap<String, EntryKind> = BTreeMap::new();
        for path in tree.keys() {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((name, _)) => entries.insert(name.into(), EntryKind::Folder),
                None => entries.insert(rest.into(), EntryKind::File),
            };
        }
        if entries.is_empty() && !folder.is_empty() {
            return Err(not_in_workspace(folder));
        }
        let mut entries: Vec<TreeEntry> = entries
            .into_iter()
            .map(|(name, kind)| TreeEntry { name, kind })
            .collect();
        entries.sort_by(|a, b| {
            let folders = (b.kind == EntryKind::Folder).cmp(&(a.kind == EntryKind::Folder));
            folders.then_with(|| natural_cmp(&a.name, &b.name))
        });
        Ok(entries)
    }

    pub fn read_workspace_file(&self, task_id: &str, path: &str) -> Answer<WorkspaceFile> {
        check_inside(path)?;
        let data = self.data();
        let file = data
            .worktree(task_id)?
            .get(path)
            .ok_or_else(|| not_in_workspace(path))?;
        Ok(WorkspaceFile {
            path: path.into(),
            text: file.text.clone(),
            binary: file.text.is_none() && !file.too_large,
            too_large: file.too_large,
            version: file.version(path),
        })
    }

    /// 409 when `version` is not the file's now.
    pub fn write_workspace_file(
        &self,
        task_id: &str,
        path: &str,
        text: String,
        version: String,
    ) -> Answer<SavedFile> {
        check_inside(path)?;
        let mut data = self.data();
        data.worktree(task_id)?;
        let file = data
            .files
            .trees
            .get_mut(task_id)
            .and_then(|tree| tree.get_mut(path))
            .ok_or_else(|| not_in_workspace(path))?;
        if file.too_large {
            return Err(status(
                413,
                format!("{path} would be larger than 1 MB, so it is not saved"),
            ));
        }
        if file.version(path) != version {
            return Err(status(
                409,
                format!("{path} changed on disk after you opened it"),
            ));
        }
        file.text = Some(text);
        file.generation += 1;
        let version = file.version(path);
        data.changed_workspace(task_id);
        Ok(SavedFile {
            path: path.into(),
            version,
        })
    }

    pub fn read_stage_file(&self, task_id: &str, name: &str) -> Answer<StageFileText> {
        let data = self.data();
        if !data.tasks.iter().any(|task| task.id == task_id) {
            return Err(not_found("Task", task_id));
        }
        let text = data
            .files
            .stage_texts
            .get(task_id)
            .and_then(|t| t.get(name));
        let text = text.ok_or_else(|| status(404, format!("{task_id} has no {name}")))?;
        Ok(StageFileText {
            name: name.into(),
            text: text.clone(),
        })
    }

    /// Stores the text, moves the file's `modifiedAt` to now, and lists a new file.
    pub fn write_stage_file(
        &self,
        task_id: &str,
        name: &str,
        text: String,
    ) -> Answer<StageFileText> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        data.files
            .stage_texts
            .entry(task_id.into())
            .or_default()
            .insert(name.into(), text.clone());
        let size = text.len() as u64;
        match data.stage_file_mut(task_id, name) {
            Some(listed) => {
                listed.size = size;
                listed.modified_at = NOW.into();
            }
            None => data
                .stage_files
                .entry(task_id.into())
                .or_default()
                .push(StageFile {
                    name: name.into(),
                    size,
                    modified_at: NOW.into(),
                }),
        }
        data.changed_workspace(task_id);
        Ok(StageFileText {
            name: name.into(),
            text,
        })
    }

    pub fn list_attachments(&self, task_id: &str) -> Answer<Vec<Attachment>> {
        let data = self.data();
        if !data.tasks.iter().any(|task| task.id == task_id) {
            return Err(not_found("Task", task_id));
        }
        Ok(data
            .files
            .attachments
            .get(task_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn read_attachment(&self, task_id: &str, name: &str) -> Answer<Vec<u8>> {
        let listed = self.list_attachments(task_id)?;
        if !listed.iter().any(|each| each.name == name) {
            return Err(status(404, format!("{task_id} has no {name}")));
        }
        Ok(attachment_bytes(name).to_vec())
    }

    /// A file of a `--shot` state that only that state shows, such as a code sample.
    pub fn put_workspace_file(&self, task_id: &str, path: &str, text: String) {
        let file = FileState {
            text: Some(text),
            too_large: false,
            generation: 1,
        };
        let mut data = self.data();
        let tree = data.files.trees.entry(task_id.into()).or_default();
        tree.insert(path.into(), file);
    }

    /// Moves a file on as if the agent wrote it: a new version, so a save with the old one gets
    /// 409. For the conflict shot.
    pub fn set_changed_on_disk(&self, task_id: &str, path: &str) {
        let mut data = self.data();
        if let Some(file) = data
            .files
            .trees
            .get_mut(task_id)
            .and_then(|tree| tree.get_mut(path))
        {
            file.generation += 1;
            data.changed_workspace(task_id);
        }
    }
}

#[cfg(test)]
#[path = "fake_files_tests.rs"]
mod tests;
