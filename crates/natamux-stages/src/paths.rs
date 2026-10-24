//! Ports `packages/daemon/src/stages/stage-paths.ts`.

use std::path::{Path, PathBuf};

use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::schema::{BranchPath, Condition, StageBranch};

use crate::checks::{OUTPUT_TAIL_CHARS, RunCommand, TIMED_OUT, section_body, tail_chars};

/// Where the conditions of a branch are checked (docs/design-task-stages.md).
#[derive(Clone, Debug)]
pub struct PathInput {
    /// The folder of the stage files: in the worktree, or the task's archive without one.
    pub folder: PathBuf,
    /// Where a command runs: the root of the worktree, or the folder without one.
    pub cwd: PathBuf,
    /// The paths the task has. The settings may have changed since the task was made.
    pub paths: Vec<String>,
}

/// The path taken (None when the task skips the branch), and why, for people.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedPath {
    pub path: Option<String>,
    pub because: String,
}

/// `trouble`: the command gave no answer. People see it even when the task takes another path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub holds: bool,
    pub because: String,
    pub trouble: bool,
}

/// A file of the stage folder has the text, under the section when one is named. Case does not
/// count. A file that is missing or cannot be read does not have it.
pub fn file_holds(file: &str, section: Option<&str>, contains: &str, folder: &Path) -> Held {
    let Ok(bytes) = std::fs::read(folder.join(file)) else {
        return Held {
            holds: false,
            because: format!("{file} is missing"),
            trouble: false,
        };
    };
    let text = String::from_utf8_lossy(&bytes);
    let body = match section {
        None => text.into_owned(),
        Some(name) => section_body(&text, name).join("\n"),
    };
    let holds = body.to_lowercase().contains(&contains.to_lowercase());
    let path = BranchPath {
        id: String::new(),
        name: None,
        when: Some(Condition::Output {
            file: file.into(),
            section: section.map(str::to_string),
            contains: contains.into(),
        }),
        not: Some(!holds),
        stages: Vec::new(),
    };
    Held {
        holds,
        because: describe_condition(&path),
        trouble: false,
    }
}

/// The command exits with code 0, run as a `check` command is. One that cannot start or times
/// out does not hold.
pub fn command_holds(run: &str, cwd: &Path, run_command: &RunCommand) -> Held {
    let ran = run_command(run, cwd);
    if ran.code == 0 {
        return Held {
            holds: true,
            because: format!("{run} exited 0"),
            trouble: false,
        };
    }
    let output = ran.output.trim_end_matches(natamux_core::js::is_js_space);
    let tail = if output.is_empty() {
        String::new()
    } else {
        format!(":\n{}", tail_chars(output, OUTPUT_TAIL_CHARS))
    };
    Held {
        holds: false,
        because: format!("{run} did not exit 0{tail}"),
        trouble: output.ends_with(TIMED_OUT),
    }
}

/// Takes the first path whose condition holds; a path without one is the Otherwise path. With
/// none, the task skips the branch. Nothing here fails the stage that ends.
pub fn pick_path(branch: &StageBranch, input: &PathInput, run_command: &RunCommand) -> PickedPath {
    let mut notes: Vec<String> = Vec::new();
    let because = |notes: &[String], last: &str| {
        notes
            .iter()
            .map(String::as_str)
            .chain([last])
            .collect::<Vec<_>>()
            .join("; ")
    };
    for path in &branch.paths {
        if !input.paths.contains(&path.id) {
            notes.push(format!("{} is not in this task", path.id));
            continue;
        }
        let Some(when) = &path.when else {
            return PickedPath {
                path: Some(path.id.clone()),
                because: because(&notes, "Otherwise"),
            };
        };
        let held = match when {
            Condition::Output {
                file,
                section,
                contains,
            } => file_holds(file, section.as_deref(), contains, &input.folder),
            Condition::Command { run } => command_holds(run, &input.cwd, run_command),
        };
        if held.holds != (path.not == Some(true)) {
            return PickedPath {
                path: Some(path.id.clone()),
                because: because(&notes, &held.because),
            };
        }
        if held.trouble {
            notes.push(held.because);
        }
    }
    PickedPath {
        path: None,
        because: because(&notes, "No condition held"),
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
