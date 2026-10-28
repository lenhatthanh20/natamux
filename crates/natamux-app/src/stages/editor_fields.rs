//! Edits of one stage's fields in the pipeline editor (`pipeline-editor/stage-fields.ts`), and the
//! defaults the catalog gives each field.

use natamux_proto::stages::capabilities::stage_name_of_id;
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::prompt::{commit_rule, default_prompt};
use natamux_proto::stages::schema::{Condition, Gate, Stage, StageKind, Step};
use natamux_proto::stages::terminal_lane::{can_have_own_agent, terminal_of_stage};

use crate::stages::editor_find::{find_stage, group_of, step_of_stage};

fn update_stage(steps: &[Step], id: &str, change: impl Fn(&mut Stage)) -> Vec<Step> {
    let mut next = steps.to_vec();
    for step in &mut next {
        match step {
            Step::Branch(branch) => {
                for stage in branch.paths.iter_mut().flat_map(|path| &mut path.stages) {
                    if stage.id == id {
                        change(stage);
                    }
                }
            }
            Step::Group(group) => {
                for stage in group.parallel.iter_mut().filter(|stage| stage.id == id) {
                    change(stage);
                }
            }
            Step::Stage(stage) => {
                if stage.id == id {
                    change(stage);
                }
            }
        }
    }
    next
}

/// A stage that is off stays in the file with its settings; new tasks do not get it.
pub fn set_enabled(steps: &[Step], id: &str, on: bool) -> Vec<Step> {
    update_stage(steps, id, |stage| {
        stage.enabled = (!on).then_some(false);
    })
}

pub fn set_gate(steps: &[Step], id: &str, gate: Gate) -> Vec<Step> {
    update_stage(steps, id, |stage| stage.gate = gate)
}

/// Tasks get a "Run with a workflow" checkbox on the stage. Off leaves the field out.
pub fn set_workflow(steps: &[Step], id: &str, on: bool) -> Vec<Step> {
    update_stage(steps, id, |stage| stage.workflow = on.then_some(true))
}

pub fn set_ci(steps: &[Step], id: &str, on: bool) -> Vec<Step> {
    update_stage(steps, id, |stage| stage.ci = on.then_some(true))
}

/// Off leaves the field out, so a stage of the catalog stays as the catalog has it.
pub fn set_end_shell(steps: &[Step], id: &str, on: bool) -> Vec<Step> {
    update_stage(steps, id, |stage| stage.end_shell = on.then_some(true))
}

/// Off keeps the stage's own agent: the stage then runs in a terminal of its own from Terminal 6.
pub fn set_own_shell(steps: &[Step], id: &str, on: bool) -> Vec<Step> {
    update_stage(steps, id, |stage| stage.own_shell = on.then_some(true))
}

/// The catalog runs its checks (Test spec, Review and the like) in a terminal of their own.
pub fn default_own_shell(id: &str) -> bool {
    catalog_entry(id).is_some_and(|entry| entry.stage.own_shell == Some(true))
}

/// Only an agent stage with an output may run in its own terminal; a group ignores it. A stage
/// that has it anyway shows it, so it can be cleared.
pub fn offers_own_shell(steps: &[Step], id: &str) -> bool {
    let Some(stage) = find_stage(steps, id) else {
        return false;
    };
    let fits = stage.kind == StageKind::Agent && stage.output.is_some();
    stage.own_shell == Some(true) || (fits && group_of(steps, id).is_none())
}

pub fn runs_in_own_shell(steps: &[Step], id: &str) -> bool {
    find_stage(steps, id).is_some_and(|stage| stage.own_shell == Some(true))
        && group_of(steps, id).is_none()
}

/// The catalog offers CI on Implement and Automated tests; a stage of your own does not.
pub fn default_ci(id: &str) -> bool {
    catalog_entry(id).is_some_and(|entry| entry.stage.ci == Some(true))
}

/// The catalog offers a workflow on its hard stages; a stage of your own does not.
pub fn default_workflow(id: &str, kind: StageKind) -> bool {
    kind == StageKind::Agent
        && catalog_entry(id).is_some_and(|entry| entry.stage.workflow == Some(true))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextField {
    Name,
    Prompt,
    Command,
    Check,
    Output,
    DoneWhen,
    Agent,
    Args,
}

fn text_slot(stage: &mut Stage, field: TextField) -> &mut Option<String> {
    match field {
        TextField::Name => &mut stage.name,
        TextField::Prompt => &mut stage.prompt,
        TextField::Command => &mut stage.command,
        TextField::Check => &mut stage.check,
        TextField::Output => &mut stage.output,
        TextField::DoneWhen => &mut stage.done_when,
        TextField::Agent => &mut stage.agent,
        TextField::Args => &mut stage.args,
    }
}

/// What a shell field holds. `Run` is the command of a path's condition; its owner is the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Check,
    Command,
    Args,
    Run,
}

/// A command a settings file would run. `id` and `name` are of the stage, or of the path for
/// `Run`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellField {
    pub id: String,
    pub name: Option<String>,
    pub field: ShellKind,
    pub value: String,
}

fn stage_fields(stage: &Stage) -> Vec<ShellField> {
    [
        (ShellKind::Check, &stage.check),
        (ShellKind::Command, &stage.command),
        (ShellKind::Args, &stage.args),
    ]
    .into_iter()
    .filter_map(|(field, value)| {
        Some(ShellField {
            id: stage.id.clone(),
            name: stage.name.clone(),
            field,
            value: value.clone()?,
        })
    })
    .collect()
}

/// What a settings file would run in a shell or put on an agent's command line, so an import can
/// show it before it lands. Stages that are off count: they can be turned on later.
pub fn shell_fields(steps: &[Step]) -> Vec<ShellField> {
    let mut found = Vec::new();
    for step in steps {
        if let Step::Branch(branch) = step {
            for path in &branch.paths {
                if let Some(Condition::Command { run }) = &path.when {
                    found.push(ShellField {
                        id: path.id.clone(),
                        name: path.name.clone(),
                        field: ShellKind::Run,
                        value: run.clone(),
                    });
                }
                found.extend(path.stages.iter().flat_map(stage_fields));
            }
        } else {
            found.extend(step.stages().into_iter().flat_map(stage_fields));
        }
    }
    found
}

/// Every agent or testing stage may pick another agent. One that would run in Terminal 1 then runs
/// in a terminal of its own.
pub fn takes_own_agent(steps: &[Step], id: &str) -> bool {
    find_stage(steps, id).is_some_and(can_have_own_agent)
}

pub fn runs_in_shell_one(steps: &[Step], id: &str) -> bool {
    step_of_stage(steps, id).is_some() && terminal_of_stage(steps, id) == "1"
}

/// The default prompt as the agent gets it: the daemon puts the commit rule in `{commit}` of a
/// default prompt only, so a prompt that starts from this one keeps the rule.
pub fn shown_default_prompt(stage: &Stage) -> String {
    default_prompt(stage).replace("{commit}", commit_rule(stage))
}

/// An empty value removes the field. A name equal to the default name and the default prompt are
/// left out too, so the file keeps only what differs.
pub fn set_text(steps: &[Step], id: &str, field: TextField, value: &str) -> Vec<Step> {
    update_stage(steps, id, |stage| {
        let is_default = match field {
            TextField::Name => value == stage_name_of_id(&stage.id),
            TextField::Prompt => {
                value == default_prompt(stage) || value == shown_default_prompt(stage)
            }
            _ => false,
        };
        let empty = value.trim().is_empty();
        *text_slot(stage, field) = (!(empty || is_default)).then(|| value.to_string());
    })
}

pub fn set_sections(steps: &[Step], id: &str, sections: &[&str]) -> Vec<Step> {
    let kept: Vec<String> = sections
        .iter()
        .map(|section| section.trim())
        .filter(|section| !section.is_empty())
        .map(str::to_string)
        .collect();
    update_stage(steps, id, |stage| {
        stage.sections = (!kept.is_empty()).then(|| kept.clone());
    })
}

/// Review and manual stages always wait for a person.
pub fn always_waits(stage: &Stage) -> bool {
    matches!(stage.kind, StageKind::Review | StageKind::Manual)
}

/// The catalog's gate; a stage of your own waits for your approval.
pub fn default_gate(id: &str, kind: StageKind) -> Gate {
    if matches!(kind, StageKind::Review | StageKind::Manual) {
        return Gate::Manual;
    }
    catalog_entry(id).map_or(Gate::Manual, |entry| entry.stage.gate)
}

pub fn is_default_gate(stage: &Stage) -> bool {
    stage.gate == default_gate(&stage.id, stage.kind)
}

pub fn has_custom_prompt(stage: &Stage) -> bool {
    stage.prompt.is_some()
}

/// Agent and testing stages get a prompt.
pub fn takes_prompt(stage: &Stage) -> bool {
    matches!(stage.kind, StageKind::Agent | StageKind::Testing)
}

#[cfg(test)]
#[path = "editor_fields_tests.rs"]
mod tests;
