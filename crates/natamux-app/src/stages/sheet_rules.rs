//! Words and rules of the stage sheet (layout-b 5.3, T-0313 mockup B): its first line, the
//! approval row, which rows and fields a stage shows, and the stages before and after it.

use natamux_proto::stages::capabilities::{can_end_shell, checks_on_ci, stage_name};
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::stages::terminal_lane::terminal_of_stage;

use super::advanced_rules::SheetGroup;
use super::editor_fields::{
    always_waits, offers_own_shell, runs_in_own_shell, runs_in_shell_one, takes_own_agent,
    takes_prompt,
};

/// A command or arguments set in the file on a stage that does not run them stay visible, so a
/// shared file hides nothing.
pub const NOT_USED: &str = "This stage does not use it. Clear it to remove it from the file.";

pub const TERMINAL_ONE: &str = "This stage runs in Terminal 1, which cannot be closed. Terminal 1 \
                                keeps the task's agent and its session.";

/// "Agent · Write what to build and why", or "A stage of your own".
pub fn sheet_intro(stage: &Stage) -> String {
    match catalog_entry(&stage.id) {
        Some(entry) => format!("{} \u{b7} {}", sheet_kind(stage.kind), entry.description),
        None => "A stage of your own".to_owned(),
    }
}

/// The kind as the sheet names it: the stages that run an agent are "Agent".
pub fn sheet_kind(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Testing => "Testing",
        StageKind::Manual => "Manual",
        StageKind::Context | StageKind::Agent | StageKind::Review | StageKind::Release => "Agent",
    }
}

/// The approval row: label, and the help line under it.
pub fn approval_text(stage: &Stage) -> (&'static str, Option<&'static str>) {
    if stage.kind == StageKind::Release {
        return (
            "Open the pull request when I click",
            Some("Unchecked, the pull request opens when the stage starts."),
        );
    }
    let help = always_waits(stage).then_some("Always on for this kind of stage.");
    ("Wait for my approval", help)
}

/// The help of the Agent row: where the stage runs.
pub fn agent_help(steps: &[Step], id: &str) -> &'static str {
    if runs_in_shell_one(steps, id) {
        "\"Task's agent\" shares Terminal 1. Another agent gets a terminal of its own."
    } else if runs_in_own_shell(steps, id) {
        "This stage runs in a terminal of its own, apart from Terminal 1."
    } else {
        "This stage starts its own agent beside Terminal 1, so it may use another."
    }
}

/// What the stage sheet shows under the Agent row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndAgent {
    /// The "End the agent when done" switch.
    Switch,
    /// The Terminal 1 line: the stage runs there, which never closes.
    TerminalOne,
    Hidden,
}

/// The rows and fields of a stage's sheet. A command or arguments set on a stage that does not
/// use them still shows, with `NOT_USED`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetShows {
    pub agent: bool,
    pub end_agent: EndAgent,
    pub prompt: bool,
    pub args: bool,
    pub args_unused: bool,
    pub command: bool,
    pub command_unused: bool,
    pub output: bool,
    pub sections: bool,
    pub workflow: bool,
    pub ci: bool,
    pub own_terminal: bool,
}

fn set(text: &Option<String>) -> bool {
    text.is_some()
}

/// Terminal 1 keeps the task's agent and its session, so endShell never closes it. A testing
/// stage with a command runs it in a terminal of its own, which it may end.
fn in_terminal_one(steps: &[Step], stage: &Stage) -> bool {
    if stage.kind == StageKind::Testing && stage.command.is_some() {
        return false;
    }
    terminal_of_stage(steps, &stage.id) == "1"
}

pub fn sheet_shows(stage: &Stage, steps: &[Step]) -> SheetShows {
    let own_agent = takes_own_agent(steps, &stage.id);
    let testing = stage.kind == StageKind::Testing;
    let agent = stage.kind == StageKind::Agent;
    let end_agent = match (can_end_shell(stage), in_terminal_one(steps, stage)) {
        (false, _) => EndAgent::Hidden,
        (true, true) => EndAgent::TerminalOne,
        (true, false) => EndAgent::Switch,
    };
    SheetShows {
        agent: own_agent,
        end_agent,
        prompt: takes_prompt(stage),
        args: own_agent || set(&stage.args),
        args_unused: !own_agent,
        command: testing || set(&stage.command),
        command_unused: !testing,
        output: agent,
        sections: agent && set(&stage.output),
        workflow: agent,
        ci: checks_on_ci(stage),
        own_terminal: offers_own_shell(steps, &stage.id),
    }
}

/// A testing stage runs its command and also takes a prompt: the command sits over the prompt
/// at the left. A command the stage does not run stays in Files and arguments.
pub fn command_at_left(shows: &SheetShows) -> bool {
    shows.prompt && shows.command && !shows.command_unused
}

/// The folding groups the right column draws: Done check and Files only for a stage with a
/// prompt (the others show their run fields in the left pane), each only when it has a row.
pub fn groups_shown(shows: &SheetShows) -> Vec<SheetGroup> {
    let command = shows.command && !command_at_left(shows);
    let files = shows.prompt && (shows.args || command || shows.output || shows.sections);
    let options = shows.workflow || shows.ci || shows.own_terminal;
    [
        (shows.prompt, SheetGroup::DoneCheck),
        (files, SheetGroup::Files),
        (options, SheetGroup::Options),
    ]
    .into_iter()
    .filter_map(|(shown, group)| shown.then_some(group))
    .collect()
}

/// A stage the stepper of the sheet goes to: its id and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageLink {
    pub id: String,
    pub name: String,
}

/// Where a stage sits in the list of all stages (in groups and paths too, list order): the one
/// before, the one after, its place from 1, and how many there are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Neighbours {
    pub prev: Option<StageLink>,
    pub next: Option<StageLink>,
    pub place: usize,
    pub count: usize,
}

pub fn stage_neighbours(steps: &[Step], id: &str) -> Option<Neighbours> {
    let stages: Vec<&Stage> = steps.iter().flat_map(Step::stages).collect();
    let index = stages.iter().position(|stage| stage.id == id)?;
    let link = |at: Option<usize>| {
        at.and_then(|at| stages.get(at)).map(|stage| StageLink {
            id: stage.id.clone(),
            name: stage_name(stage),
        })
    };
    Some(Neighbours {
        prev: link(index.checked_sub(1)),
        next: link(Some(index + 1)),
        place: index + 1,
        count: stages.len(),
    })
}

#[cfg(test)]
#[path = "sheet_rules_tests.rs"]
mod tests;
