//! What one stage card shows (`StageCard.tsx`, `StageActions`, `StageNotices`): its chip, its
//! buttons in order, and its notices. Pure, so the views only draw what this says.

use natamux_proto::events::task_event::WaitReason;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Stage, StageKind};
use natamux_proto::stages::terminal_lane::can_have_own_agent;
use natamux_proto::tasks::task::{Task, TaskState};

/// The chip after a card's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardChip {
    /// The state of the current stage.
    State(TaskState),
    Done,
    Skipped,
    WillBeSkipped,
    NotStarted,
    NotTaken,
}

/// A button of the current stage, in the order the card shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardButton {
    /// First and wide. `anyway`: the stage needs input, and the key does not approve it.
    Approve {
        anyway: bool,
    },
    AnswerInTerminal,
    Resume,
    PickAgent,
    StartAgent,
    MarkAsDone,
    /// Not a button: what finishes a stage that waits for a person or a pull request.
    FinishedWhen,
    Start,
    Skip,
    SendBack,
}

/// A box under the chips of the current stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardNotice {
    /// The reasons of the last checks, or of the agent's failure. `failed`: in the failed tone.
    Reasons { failed: bool },
    /// Why the stage waits, when no reasons say it.
    Wait(WaitReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardFacts {
    pub chip: CardChip,
    pub buttons: Vec<CardButton>,
    pub notices: Vec<CardNotice>,
}

/// `STARTS_ITSELF`: review, manual and release stages wait for a person or a pull request.
pub fn starts_itself(kind: StageKind) -> bool {
    matches!(
        kind,
        StageKind::Context | StageKind::Agent | StageKind::Testing
    )
}

/// `marked`: a stage not reached yet that the task will skip.
pub fn card_chip(entry: &StageProgress, marked: bool) -> CardChip {
    match (entry.status, entry.state) {
        (StageStatus::Current, Some(state)) => CardChip::State(state),
        (StageStatus::Done, _) => CardChip::Done,
        (StageStatus::Skipped, _) => CardChip::Skipped,
        (StageStatus::NotTaken, _) => CardChip::NotTaken,
        (StageStatus::Upcoming, _) if marked => CardChip::WillBeSkipped,
        _ => CardChip::NotStarted,
    }
}

/// The buttons of the current stage (`StageActions`). `asked`: the wait is a question;
/// `can_answer`: the task's terminal is beside the panel.
pub fn card_buttons(
    entry: &StageProgress,
    kind: StageKind,
    asked: bool,
    can_answer: bool,
) -> Vec<CardButton> {
    if entry.status != StageStatus::Current {
        return Vec::new();
    }
    let state = entry.state;
    let is = |states: &[TaskState]| state.is_some_and(|state| states.contains(&state));
    let mut buttons = Vec::new();
    if is(&[TaskState::Review, TaskState::NeedsInput]) {
        buttons.push(CardButton::Approve {
            anyway: state == Some(TaskState::NeedsInput),
        });
    }
    let asking = state == Some(TaskState::NeedsInput);
    if asking && asked && can_answer {
        buttons.push(CardButton::AnswerInTerminal);
    }
    if asking {
        buttons.push(CardButton::Resume);
    }
    if is(&[TaskState::Failed, TaskState::NeedsInput]) && starts_itself(kind) {
        buttons.extend([CardButton::PickAgent, CardButton::StartAgent]);
    }
    if is(&[TaskState::Running, TaskState::NeedsInput]) {
        buttons.push(CardButton::MarkAsDone);
    }
    if is(&[TaskState::Draft, TaskState::Ready]) {
        buttons.push(if starts_itself(kind) {
            CardButton::Start
        } else {
            CardButton::FinishedWhen
        });
    }
    buttons.extend([CardButton::Skip, CardButton::SendBack]);
    buttons
}

/// The boxes of the current stage (`StageNotices`): the reasons while the agent does not fix
/// them, else why a stage that needs input or failed waits.
pub fn card_notices(entry: &StageProgress, wait: &WaitReason) -> Vec<CardNotice> {
    if entry.status != StageStatus::Current {
        return Vec::new();
    }
    let failed = entry.state == Some(TaskState::Failed);
    if !entry.reasons.is_empty() {
        if entry.state == Some(TaskState::Running) {
            return Vec::new();
        }
        return vec![CardNotice::Reasons { failed }];
    }
    if !matches!(entry.state, Some(TaskState::NeedsInput | TaskState::Failed)) {
        return Vec::new();
    }
    // A failed stage whose events do not say why still says it failed.
    let own_reason = matches!(
        wait,
        WaitReason::Failed { .. } | WaitReason::CheckFailed { .. }
    );
    let reason = if failed && !own_reason {
        WaitReason::Failed { reason: None }
    } else {
        wait.clone()
    };
    vec![CardNotice::Wait(reason)]
}

/// Everything a card shows. `can_act`: the task is not finished, so the current stage has its
/// buttons.
pub fn card_facts(
    entry: &StageProgress,
    kind: StageKind,
    marked: bool,
    wait: &WaitReason,
    can_answer: bool,
    can_act: bool,
) -> CardFacts {
    let asked = matches!(wait, WaitReason::Asked { .. });
    CardFacts {
        chip: card_chip(entry, marked),
        buttons: if can_act {
            card_buttons(entry, kind, asked, can_answer)
        } else {
            Vec::new()
        },
        notices: card_notices(entry, wait),
    }
}

/// A done task's done stages with no end of their own ended with the task (`ProgressTab.tsx`).
pub fn end_with_the_task(progress: &mut [StageProgress], task: &Task) {
    if task.state != TaskState::Done {
        return;
    }
    let open = progress
        .iter_mut()
        .filter(|entry| entry.status == StageStatus::Done && entry.ended_at.is_none());
    for entry in open {
        entry.ended_at = Some(task.updated_at.clone());
    }
}

/// A piece of a reason (`ReasonText`): plain text, or an address shown as a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReasonPart {
    Text(String),
    /// A CI run's address shows as "view run", so the reason stays on one line.
    Link {
        url: String,
        shown: String,
    },
}

/// The punctuation that ends a sentence or closes a bracket is not part of the address.
const NOT_LAST: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"'];

fn is_run(url: &str) -> bool {
    url.split("/actions/runs/")
        .nth(1)
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// Splits a reason at its http and https addresses.
pub fn reason_parts(text: &str) -> Vec<ReasonPart> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(at) = [rest.find("https://"), rest.find("http://")]
        .into_iter()
        .flatten()
        .min()
    {
        let tail = &rest[at..];
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let url = tail[..end].trim_end_matches(NOT_LAST);
        let scheme = if url.starts_with("https://") { 8 } else { 7 };
        if url.len() <= scheme {
            parts.push(ReasonPart::Text(rest[..at + scheme].to_owned()));
            rest = &rest[at + scheme..];
            continue;
        }
        if at > 0 {
            parts.push(ReasonPart::Text(rest[..at].to_owned()));
        }
        let shown = if is_run(url) {
            "view run".to_owned()
        } else {
            url.to_owned()
        };
        parts.push(ReasonPart::Link {
            url: url.to_owned(),
            shown,
        });
        rest = &rest[at + url.len()..];
    }
    if !rest.is_empty() {
        parts.push(ReasonPart::Text(rest.to_owned()));
    }
    parts
}

/// The agent icon of a node: the stage's own agent, else the task's agent on a stage that runs
/// one (agent and testing stages). `true`: it is the task's agent.
pub fn node_agent(stage: &Stage, task_agent: &str) -> Option<(String, bool)> {
    match &stage.agent {
        Some(agent) => Some((agent.clone(), false)),
        None => can_have_own_agent(stage).then(|| (task_agent.to_owned(), true)),
    }
}

/// The selected node of the Progress flow: the one the user picked, else the current stage.
pub fn selected_stage(picked: Option<&str>, progress: &[StageProgress]) -> Option<String> {
    picked.map(str::to_owned).or_else(|| {
        progress
            .iter()
            .find(|entry| entry.status == StageStatus::Current)
            .map(|entry| entry.stage.clone())
    })
}

/// The pick after a click on `clicked`: a click on the selected node clears the pick, so the
/// selection goes back to the current stage; any other node becomes the pick.
pub fn pick_after_click(
    picked: Option<&str>,
    progress: &[StageProgress],
    clicked: &str,
) -> Option<String> {
    if selected_stage(picked, progress).as_deref() == Some(clicked) {
        None
    } else {
        Some(clicked.to_owned())
    }
}

#[cfg(test)]
#[path = "card_rules_tests.rs"]
mod tests;
