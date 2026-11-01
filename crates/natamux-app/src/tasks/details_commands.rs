//! What a click in the details panel asks for. The tabs and cards are drawn as plain parts;
//! the panel (`tasks/details_panel.rs`) runs each command.

use std::rc::Rc;

use gpui::{App, Window};

/// A setting of one stage that the task may change (`PUT /tasks/:id/<kind>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageToggle {
    Workflow,
    Ci,
    Skip,
}

impl StageToggle {
    /// The route's last part, and the fake's name of the setting.
    pub fn name(self) -> &'static str {
        match self {
            StageToggle::Workflow => "workflow",
            StageToggle::Ci => "ci",
            StageToggle::Skip => "skip",
        }
    }

    /// `FAILED_ACTION` and the box errors of `StageChoices.tsx`.
    pub fn failure(self) -> &'static str {
        match self {
            StageToggle::Workflow => "Could not change the workflow of the stage.",
            StageToggle::Ci => "Could not change the CI check of the stage.",
            StageToggle::Skip => "Could not change the skip of the stage.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelCommand {
    // The current stage card (`StageActions`)
    Approve(String),
    Report(String),
    Skip(String),
    Start(String),
    SendBack(String),
    AnswerInTerminal,
    Resume,
    /// The agent picker of a card.
    PickAgent(String),
    StartStageAgent,
    Toggle {
        kind: StageToggle,
        stage: String,
        on: bool,
    },
    /// Selects a node of the Progress flow; on the selected node, goes back to the current stage.
    SelectStage(String),
    /// Opens or folds "Show evidence" of a card.
    FoldEvidence(String),
    FoldLane(String),
    CreateIssue,
    // The Details tab
    /// Edit task of the "..." menu: the edit form, as `e`.
    Edit,
    SaveTemplate,
    /// A state action of the action row, by its event type.
    TaskEvent(&'static str),
    StartAgent,
    CreateWorkspace,
    PickBase(String),
    RemoveWorkspace,
    /// The output file chip of a stage card: the Docs tab with that file.
    OpenStageFile(String),
    /// Opens the Changes tab of the task's workspace.
    ViewChanges,
    CheckPullRequest,
    AddSubtask,
    KeepTry(String),
    OpenTask(String),
    CopyText {
        text: String,
        what: &'static str,
    },
    // Both tabs
    FoldSection {
        id: &'static str,
        open: bool,
    },
}

pub type OnCommand = Rc<dyn Fn(PanelCommand, &mut Window, &mut App)>;
