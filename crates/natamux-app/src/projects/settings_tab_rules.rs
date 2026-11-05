//! The tabs of Project settings as rules (`ProjectSettingsDialog.tsx`, `IssuesTab.tsx`): which tab
//! shows, the footer note, the GitHub issues choice, and the words of the reset and the JSON alert. No
//! gpui, so it is tested alone.

use natamux_proto::projects::repo::{PAUSE_IDLE_MINUTES, TrackerSettings};
use natamux_proto::projects::settings_rules::branch_name_problem;

use crate::components::input_rules::segment_step;
use crate::projects::settings_text::CREATE_ISSUE_OPTIONS;
use crate::stages::editor_draft::DEFAULT_CREATE_ISSUE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Stages,
    Json,
}

impl SettingsTab {
    /// In the order they are drawn.
    pub const ALL: [SettingsTab; 3] =
        [SettingsTab::General, SettingsTab::Stages, SettingsTab::Json];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Stages => "Stages",
            SettingsTab::Json => "JSON",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }
}

/// JSON the Stages tab cannot read keeps the person on the JSON tab.
pub fn shown_tab(draft_ok: bool, picked: SettingsTab) -> SettingsTab {
    if draft_ok { picked } else { SettingsTab::Json }
}

pub const JSON_FIRST: &str = "Fix the JSON first";

/// Why a tab cannot be picked: General and Stages need text that parses.
pub fn tab_blocked(tab: SettingsTab, draft_ok: bool) -> Option<&'static str> {
    (!draft_ok && tab != SettingsTab::Json).then_some(JSON_FIRST)
}

/// The status at the left of the bottom bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Unsaved,
    Saved,
}

impl SaveStatus {
    pub fn label(self) -> &'static str {
        match self {
            SaveStatus::Unsaved => "Unsaved changes",
            SaveStatus::Saved => "Saved",
        }
    }
}

/// A broken file counts as unsaved: Save replaces it.
pub fn save_status(dirty: bool, file_broken: bool) -> SaveStatus {
    if dirty || file_broken {
        SaveStatus::Unsaved
    } else {
        SaveStatus::Saved
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    Reset,
    Import,
    Export,
}

/// An entry of the bottom bar's "..." menu; `icon` is a Lucide name for `icon_path`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMenuEntry {
    Item {
        action: FileAction,
        label: &'static str,
        icon: &'static str,
        disabled: bool,
    },
    Label(&'static str),
}

/// Reset to default, Import… and Export; text that cannot be read turns Export off, with the
/// reason above it.
pub fn file_menu(draft_ok: bool) -> Vec<FileMenuEntry> {
    let item = |action, label, icon, disabled| FileMenuEntry::Item {
        action,
        label,
        icon,
        disabled,
    };
    let mut entries = vec![
        item(FileAction::Reset, "Reset to default", "rotate-ccw", false),
        item(FileAction::Import, "Import\u{2026}", "download", false),
    ];
    let cannot_export = export_reason(draft_ok);
    entries.extend(cannot_export.map(FileMenuEntry::Label));
    entries.push(item(
        FileAction::Export,
        "Export",
        "upload",
        cannot_export.is_some(),
    ));
    entries
}

/// A radio row of the issues group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IssueRow {
    pub value: &'static str,
    pub label: &'static str,
    pub selected: bool,
    pub is_default: bool,
    /// The muted text at the right end.
    pub note: Option<&'static str>,
    pub disabled: bool,
}

/// The four choices of `tracker.createIssue`; all off while the project has no tracker.
pub fn issue_rows(create_issue: &str, off: bool) -> Vec<IssueRow> {
    let selected = choice_index(create_issue);
    CREATE_ISSUE_OPTIONS
        .iter()
        .enumerate()
        .map(|(index, &(value, label))| {
            let is_default = value == DEFAULT_CREATE_ISSUE;
            IssueRow {
                value,
                label,
                selected: index == selected,
                is_default,
                note: is_default.then_some("Button on the task"),
                disabled: off,
            }
        })
        .collect()
}

/// The footer note is about the pipeline, so General has none.
// shortcut: option B moves the note to the Stages group footnote (stages/*); drop this once
// nothing reads it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn shows_pipeline_note(tab: SettingsTab) -> bool {
    tab != SettingsTab::General
}

// shortcut: read by the Stages group footnote (stages/*) in option B; drop the allow once it is.
#[cfg_attr(not(test), allow(dead_code))]
pub const PIPELINE_NOTE: &str = "New tasks use this pipeline. Tasks on their way keep their stages, \
                                 but use the new prompts and approvals.";

/// The tab an arrow key moves to from `shown`.
// shortcut: `Segmented` moves with the arrows itself now; drop this with its test once agreed.
#[cfg_attr(not(test), allow(dead_code))]
pub fn step_tab(shown: SettingsTab, key: &str) -> Option<SettingsTab> {
    let next = segment_step(SettingsTab::ALL.len(), shown.index(), key)?;
    SettingsTab::ALL.get(next).copied()
}

/// The choice an arrow key moves to from `selected` (an index of `CREATE_ISSUE_OPTIONS`).
pub fn step_choice(selected: usize, key: &str) -> Option<usize> {
    segment_step(CREATE_ISSUE_OPTIONS.len(), selected, key)
}

/// The index of the choice a project has; the default (Manual) for a value the list lacks.
pub fn choice_index(create_issue: &str) -> usize {
    CREATE_ISSUE_OPTIONS
        .iter()
        .position(|(value, _)| *value == create_issue)
        .unwrap_or(0)
}

/// A project with `tracker.type` none opens no issue, so the choice is shown but off.
pub fn issues_off(tracker: &TrackerSettings) -> bool {
    tracker.kind == "none"
}

pub const ISSUES_OFF_NOTE: &str = "Issues are off: tracker.type is none (JSON tab)";

/// Why Export is off, or None. Pipeline problems do not block it, only text it cannot read.
pub fn export_reason(draft_ok: bool) -> Option<&'static str> {
    (!draft_ok).then_some(JSON_FIRST)
}

/// The problems of the pipeline, plus one for text that cannot be read.
pub fn error_count(draft_ok: bool, pipeline_errors: usize) -> usize {
    pipeline_errors + usize::from(!draft_ok)
}

/// The title, the question and the new text of Reset to default.
pub struct ResetAsk {
    pub question: &'static str,
    /// The new text, or None: put the default stages back in the draft.
    pub text: Option<&'static str>,
}

pub fn reset_ask(draft_ok: bool) -> ResetAsk {
    if draft_ok {
        ResetAsk {
            question: "Put the default stages back? Other settings stay as they are. Nothing is \
                       saved until you click Save.",
            text: None,
        }
    } else {
        ResetAsk {
            question: "Replace the text with the default settings?",
            text: Some("{}"),
        }
    }
}

pub const SEND_BACKS_LABEL: &str = "Agent send backs";
pub const SEND_BACKS_HINT: &str =
    "How many times an agent can send a stage back. Empty means no limit.";
pub const SEND_BACKS_ERROR: &str = "Give a whole number of 0 or more, or leave it empty.";

/// The text of the send backs field: empty is no limit. Digits only, as JSON writes the number.
pub fn send_backs_field(text: &str) -> Result<Option<usize>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(SEND_BACKS_ERROR);
    }
    text.parse().map(Some).map_err(|_| SEND_BACKS_ERROR)
}

pub const BRANCH_NAME_LABEL: &str = "Branch name";
pub const BRANCH_NAME_PLACEHOLDER: &str = "natamux/{id}-{slug}";
pub const BRANCH_NAME_HINT: &str = "Use {id} and {slug}. New tasks use it for their branch.";

/// The text of the branch name field: empty is None (the key is left out), else the trimmed text
/// or the problem of `git.branchName`.
pub fn branch_name_field(text: &str) -> Result<Option<String>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match branch_name_problem(text) {
        Some(problem) => Err(problem),
        None => Ok(Some(text.to_string())),
    }
}

pub const IDLE_PAUSE_LABEL: &str = "Pause idle agents after (minutes)";
pub const IDLE_PAUSE_HINT: &str = "0 turns it off. Otherwise 5 to 1440.";
pub const IDLE_PAUSE_ERROR: &str = "Give 0 (off) or a whole number from 5 to 1440, or leave it \
                                    empty.";

/// The text of the idle pause field: empty is the default, 30 minutes.
pub fn idle_pause_field(text: &str) -> Result<Option<u64>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(IDLE_PAUSE_ERROR);
    }
    match text.parse::<u64>() {
        Ok(minutes) if minutes == 0 || PAUSE_IDLE_MINUTES.contains(&minutes) => Ok(Some(minutes)),
        _ => Err(IDLE_PAUSE_ERROR),
    }
}

/// The alert under the JSON editor.
pub fn json_alert(error: &str) -> String {
    format!("{error} Fix it to use the Stages tab.")
}

/// A pipeline problem as the JSON tab lists it: "<stage>: <message>".
pub fn problem_line(stage: Option<&str>, message: &str) -> String {
    match stage {
        Some(stage) => format!("{stage}: {message}"),
        None => message.to_owned(),
    }
}

#[cfg(test)]
#[path = "settings_tab_rules_tests.rs"]
mod tests;
