//! The tabs of Project settings as rules (`ProjectSettingsDialog.tsx`, `IssuesTab.tsx`): which tab
//! shows, the footer note, the Issues choice, and the words of the reset and the JSON alert. No
//! gpui, so it is tested alone.

use natamux_proto::projects::repo::TrackerSettings;

use crate::components::input_rules::segment_step;
use crate::projects::settings_text::CREATE_ISSUE_OPTIONS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Stages,
    Issues,
    Json,
}

impl SettingsTab {
    /// In the order they are drawn.
    pub const ALL: [SettingsTab; 3] = [SettingsTab::Stages, SettingsTab::Issues, SettingsTab::Json];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::Stages => "Stages",
            SettingsTab::Issues => "Issues",
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

/// The footer note is about the pipeline, so Issues has none.
pub fn shows_pipeline_note(tab: SettingsTab) -> bool {
    tab != SettingsTab::Issues
}

pub const PIPELINE_NOTE: &str = "New tasks use this pipeline. Tasks on their way keep their stages, \
                                 but use the new prompts and approvals.";

/// The tab an arrow key moves to from `shown`.
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
    (!draft_ok).then_some("Fix the JSON first")
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
