//! The rules of a pipeline (`pipeline-schema.ts` and `pipeline-issues.ts` in
//! `packages/core/src/stages`), as the messages the settings screen shows next to a stage.
//!
//! The rules work on steps that were read, so a field of a wrong JSON type, and a group or a
//! branch inside a path, are refused at read time, not shown here.

use std::collections::HashSet;

use crate::stages::capabilities::{can_end_shell, checks_on_ci};
use crate::stages::catalog::catalog_entry;
use crate::stages::field_rules::name_problem;
pub use crate::stages::field_rules::{condition_problems, stage_id_problem, stage_problems};
use crate::stages::resolve::active_steps;
use crate::stages::schema::{GROUP_KINDS, Stage, StageBranch, StageGroup, StageKind, Step};
use crate::stages::terminal_lane::can_have_own_agent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueLevel {
    Error,
    Warning,
}

/// A problem of a pipeline that the settings screen shows next to its stage. Errors stop a save;
/// warnings do not. `stage`: the stage or group it is about, or None for the whole pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineIssue {
    pub stage: Option<String>,
    pub message: String,
    pub level: IssueLevel,
}

/// One problem of `pipelineSchema`: the stage it names, and the message.
type Found = (Option<String>, String);

const GROUP_SIZE: &str = "A group holds 2 to 4 stages";
const BRANCH_SIZE: &str = "A branch holds 2 to 4 paths";

fn field_label(field: &str) -> &str {
    match field {
        "id" => "Id",
        "name" => "Name",
        "prompt" => "Prompt",
        "output" => "Output file",
        "sections" => "Required sections",
        "command" => "Command",
        "check" => "Check command",
        "doneWhen" => "Finished when",
        "agent" => "Agent",
        "args" => "Arguments",
        other => other,
    }
}

/// The rules of `pipelineSchema` and of the catalog, as one message per problem. Each stage is
/// checked on its own first, so a wrong field names its stage instead of the whole step.
pub fn pipeline_issues(steps: &[Step]) -> Vec<PipelineIssue> {
    let error = |stage: Option<String>, message: String| PipelineIssue {
        stage,
        message,
        level: IssueLevel::Error,
    };
    let mut issues = Vec::new();
    for stage in steps.iter().flat_map(Step::stages) {
        for (field, message) in stage_problems(stage) {
            let message = format!("{}: {message}", field_label(field));
            issues.push(error(Some(stage.id.clone()), message));
        }
        let needs_command = stage.kind == StageKind::Testing
            && catalog_entry(&stage.id).is_some_and(|entry| entry.needs_command);
        let no_command = stage.command.as_deref().is_none_or(str::is_empty);
        if stage.is_on() && needs_command && no_command {
            issues.push(error(
                Some(stage.id.clone()),
                "Give the command to run".into(),
            ));
        }
    }
    if issues.is_empty() {
        issues.extend(
            pipeline_problems(steps)
                .into_iter()
                .map(|(stage, message)| error(stage, message)),
        );
    }
    for step in steps {
        let Step::Group(group) = step else {
            continue;
        };
        let writers = group
            .parallel
            .iter()
            .filter(|stage| {
                stage.is_on() && stage.kind == StageKind::Agent && stage.output.is_none()
            })
            .count();
        if writers > 1 {
            issues.push(PipelineIssue {
                stage: Some(group.id.clone()),
                message: format!(
                    "{writers} stages without an output file change code in one worktree at the same time"
                ),
                level: IssueLevel::Warning,
            });
        }
    }
    issues
}

/// Every issue of `pipelineSchema.safeParse(steps)`: the shape of each step first, then the rules
/// across steps.
pub(crate) fn pipeline_problems(steps: &[Step]) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    if steps.is_empty() {
        found.push((None, "A pipeline has at least one stage".into()));
    }
    for step in steps {
        found.extend(shape_problems(step));
    }
    let mut rules = Rules {
        seen: HashSet::new(),
        found: Vec::new(),
    };
    for step in steps {
        rules.step(step);
    }
    found.extend(rules.found);
    found.extend(order_problems(steps));
    found
}

fn named(stage: &str, problem: Option<String>) -> Option<Found> {
    problem.map(|message| (Some(stage.to_string()), message))
}

fn stage_shape(stage: &Stage) -> Vec<Found> {
    stage_problems(stage)
        .into_iter()
        .map(|(_, message)| (Some(stage.id.clone()), message))
        .collect()
}

/// What `stepSchema` says about one step: sizes, kinds and the fields of its parts.
fn shape_problems(step: &Step) -> Vec<Found> {
    match step {
        Step::Stage(stage) => stage_shape(stage),
        Step::Group(group) => group_shape(group),
        Step::Branch(branch) => branch_shape(branch),
    }
}

fn group_shape(group: &StageGroup) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let id = group.id.as_str();
    found.extend(named(id, stage_id_problem(id)));
    found.extend(named(id, name_problem(group.name.as_deref())));
    for stage in &group.parallel {
        found.extend(stage_shape(stage));
        if !GROUP_KINDS.contains(&stage.kind) {
            found.push((
                Some(stage.id.clone()),
                "Only agent and manual stages can run side by side".into(),
            ));
        }
    }
    if group.parallel.len() < 2 || group.parallel.len() > 4 {
        found.push((Some(id.into()), GROUP_SIZE.into()));
    }
    found
}

fn branch_shape(branch: &StageBranch) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let id = branch.id.as_str();
    found.extend(named(id, stage_id_problem(id)));
    found.extend(named(id, name_problem(branch.name.as_deref())));
    for path in &branch.paths {
        found.extend(named(id, stage_id_problem(&path.id)));
        found.extend(named(id, name_problem(path.name.as_deref())));
        for (_, message) in path.when.iter().flat_map(condition_problems) {
            found.push((Some(id.into()), message));
        }
        if path.stages.is_empty() {
            found.push((Some(id.into()), "A path has at least one stage".into()));
        }
        for stage in &path.stages {
            found.extend(stage_shape(stage));
        }
    }
    if branch.paths.len() < 2 || branch.paths.len() > 4 {
        found.push((Some(id.into()), BRANCH_SIZE.into()));
    }
    found
}

/// The problems found so far, and the ids seen so far: a stage, a group, a branch and a path may
/// not share an id.
struct Rules {
    seen: HashSet<String>,
    found: Vec<Found>,
}

impl Rules {
    fn fail(&mut self, stage: &str, message: String) {
        self.found.push((Some(stage.to_string()), message));
    }

    fn unique(&mut self, id: &str, message: String, stage: &str) {
        if !self.seen.insert(id.to_string()) {
            self.fail(stage, message);
        }
    }

    /// `checkStepRules` for one step: unique ids, and what each stage may have.
    fn step(&mut self, step: &Step) {
        let id = step.id();
        self.unique(id, format!("The stage id \"{id}\" is used twice"), id);
        if let Step::Branch(branch) = step {
            self.branch_paths(branch);
        }
        for (stage, at, whole) in places(step) {
            if !matches!(step, Step::Stage(_)) {
                let message = format!("The stage id \"{}\" is used twice", stage.id);
                self.unique(&stage.id, message, whole);
            }
            self.found.extend(stage_rules(step, stage, at, whole));
        }
        if let Step::Group(group) = step
            && group.parallel.iter().filter(|stage| stage.is_on()).count() == 1
        {
            self.fail(
                &group.id,
                "A group needs 2 stages that are on. Turn on another, or run it on its own".into(),
            );
        }
    }

    fn branch_paths(&mut self, branch: &StageBranch) {
        let last = branch.paths.len().saturating_sub(1);
        for (index, path) in branch.paths.iter().enumerate() {
            let message = format!("The path id \"{}\" is used twice", path.id);
            self.unique(&path.id, message, &branch.id);
            if path.when.is_none() && index != last {
                let message = "Only the last path can be the Otherwise path".into();
                self.fail(&branch.id, message);
            }
        }
        // A path with all stages off is left out of new tasks. Only an Otherwise path can be the
        // one left, since its stages then run as plain steps.
        let filled: Vec<_> = branch
            .paths
            .iter()
            .filter(|path| !path.stages.is_empty())
            .collect();
        let left: Vec<_> = filled
            .iter()
            .filter(|path| path.stages.iter().any(Stage::is_on))
            .collect();
        if filled.len() >= 2 && left.len() == 1 && left[0].when.is_some() {
            let message = "Turn on a stage in another path, or remove the branch".into();
            self.fail(&branch.id, message);
        }
    }
}

/// Each stage of a step with the stage that names a problem about it (`at`: its lane or its place
/// in a path) and the stage that names a rule that is about the whole step (`whole`).
fn places(step: &Step) -> Vec<(&Stage, &str, &str)> {
    match step {
        Step::Stage(stage) => vec![(stage, stage.id.as_str(), stage.id.as_str())],
        Step::Group(group) => group
            .parallel
            .iter()
            .map(|stage| (stage, stage.id.as_str(), group.id.as_str()))
            .collect(),
        Step::Branch(branch) => branch
            .paths
            .iter()
            .flat_map(|path| &path.stages)
            .map(|stage| (stage, stage.id.as_str(), stage.id.as_str()))
            .collect(),
    }
}

/// `checkStageRules` without the unique ids.
fn stage_rules(step: &Step, stage: &Stage, at: &str, whole: &str) -> Vec<Found> {
    let id = &stage.id;
    let mut found: Vec<Found> = Vec::new();
    let mut fail = |stage: &str, message: String| found.push((Some(stage.to_string()), message));
    if stage.own_shell == Some(true) && (stage.kind != StageKind::Agent || stage.output.is_none()) {
        fail(
            at,
            format!(
                "Only an agent stage with an output file can run in its own terminal, and \"{id}\" cannot"
            ),
        );
    }
    let own_agent = stage.agent.is_some() || stage.args.is_some();
    let kind = stage.kind.as_str();
    if own_agent && !can_have_own_agent(stage) {
        fail(
            at,
            format!(
                "Only an agent or testing stage can have its own agent or arguments, and \"{id}\" is a {kind} stage"
            ),
        );
    }
    if stage.ci == Some(true) && !checks_on_ci(stage) {
        fail(
            whole,
            format!(
                "Only a stage that changes code or runs tests can be checked on CI, and \"{id}\" cannot"
            ),
        );
    }
    if stage.end_shell == Some(true) && !can_end_shell(stage) {
        fail(
            whole,
            format!(
                "Only a stage that starts an agent or a command can end its terminal, and \"{id}\" is a {kind} stage"
            ),
        );
    }
    if stage.workflow == Some(true) && stage.kind != StageKind::Agent {
        fail(
            whole,
            format!("Only agent stages can run with a workflow, and \"{id}\" is a {kind} stage"),
        );
    }
    // The context comes first and the release last, so neither can be in a path.
    let in_path = step.is_branch();
    if in_path && stage.is_on() && matches!(stage.kind, StageKind::Context | StageKind::Release) {
        fail(
            at,
            format!("\"{id}\" is a {kind} stage, which cannot be in a path"),
        );
    }
    found
}

/// `checkBranchOrder` and `checkContextAndRelease`: what counts are the steps that have a stage
/// that is on.
fn order_problems(steps: &[Step]) -> Vec<Found> {
    let on: Vec<&Step> = steps
        .iter()
        .filter(|step| step.stages().iter().any(|stage| stage.is_on()))
        .collect();
    let Some(first) = on.first() else {
        return vec![(None, "Turn on at least one stage".into())];
    };
    let mut found: Vec<Found> = Vec::new();
    let mut fail =
        |step: &Step, message: &str| found.push((Some(step.id().into()), message.into()));
    // A branch needs the end of a stage before it to check its conditions.
    if first.is_branch() {
        fail(first, "A branch cannot be the first step");
    }
    let branch_on = |step: &Step| {
        active_steps(std::slice::from_ref(step))
            .iter()
            .any(Step::is_branch)
    };
    for pair in on.windows(2) {
        if branch_on(pair[0]) && branch_on(pair[1]) {
            fail(
                pair[1],
                "A branch cannot come right after a branch. Put a stage between them",
            );
        }
    }
    // The context is read when a task is made, so only the first step can be one. A release ends
    // the task, so nothing can come after it.
    let of_kind = |kind: StageKind| -> Vec<&Step> {
        on.iter()
            .copied()
            .filter(|step| matches!(step, Step::Stage(stage) if stage.kind == kind))
            .collect()
    };
    let (contexts, releases) = (of_kind(StageKind::Context), of_kind(StageKind::Release));
    let same = |a: &Step, b: Option<&&Step>| b.is_some_and(|b| std::ptr::eq(a, *b));
    if contexts.len() > 1 {
        fail(contexts[1], "A pipeline has at most one context stage");
    } else if let Some(context) = contexts.first()
        && !same(context, on.first())
    {
        fail(context, "The context stage must come first");
    }
    if releases.len() > 1 {
        fail(releases[1], "A pipeline has at most one release stage");
    } else if let Some(release) = releases.first()
        && !same(release, on.last())
    {
        fail(release, "The release stage must come last");
    }
    found
}

#[cfg(test)]
#[path = "pipeline_branch_tests.rs"]
mod branches;
#[cfg(test)]
#[path = "pipeline_issues_tests.rs"]
mod issue_tests;
#[cfg(test)]
#[path = "pipeline_rules_tests.rs"]
mod tests;
