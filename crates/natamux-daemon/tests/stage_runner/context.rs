//! `describe('context')` of `stage-runner.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use natamux_proto::projects::repo::{ContextSettings, ContextSource};
use natamux_proto::stages::schema::{Stage, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::checks::Ran;
use natamux_store::task_files::archive_dir;
use serde_json::{Value, json};

use natamux_daemon::stages::act::StageActionInput;

use crate::{Fixture, wait_for};

/// `writeWorking`: a stage file in the task's workspace.
pub fn write_working(task: &Task, name: &str, text: &str) {
    let dir = Path::new(task.workspace.as_deref().unwrap_or("")).join(".natamux-task");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(name), text).unwrap();
}

pub fn context_of(f: &Fixture, id: &str) -> String {
    fs::read_to_string(archive_dir(&f.data_dir, id).join("context.md")).unwrap()
}

/// A skill of the user, as the home folder of the fixture keeps it.
pub fn make_skill(base: &Path, name: &str) -> PathBuf {
    // As the agents' skill folders are named (".claude/skills"), so Windows gives the same path.
    let skill = base.join(".claude/skills").join(name);
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
    skill
}

pub fn skill_source(name: &str, skill: &str) -> ContextSource {
    ContextSource {
        kind: "skill".into(),
        name: Some(name.into()),
        skill: Some(skill.into()),
        agent: None,
        prompt: None,
    }
}

pub fn manual_source() -> ContextSource {
    ContextSource {
        kind: "manual".into(),
        name: None,
        skill: None,
        agent: None,
        prompt: None,
    }
}

pub fn set_sources(f: &Fixture, sources: Vec<ContextSource>) {
    let mut settings = f.settings.lock().unwrap();
    settings.context = ContextSettings { sources };
}

/// The payloads of the events of one type.
pub fn payloads_of(f: &Fixture, id: &str, kind: &str) -> Vec<Value> {
    f.events(id)
        .into_iter()
        .filter(|(event, _)| event == kind)
        .map(|(_, payload)| payload)
        .collect()
}

/// The last `stage.started` of a stage.
pub fn last_start(f: &Fixture, id: &str, stage: &str) -> Value {
    payloads_of(f, id, "stage.started")
        .into_iter()
        .rfind(|payload| payload["stage"] == stage)
        .unwrap()
}

pub fn from_terminal(terminal: &str) -> StageActionInput {
    StageActionInput {
        terminal: Some(terminal.into()),
        ..StageActionInput::default()
    }
}

/// `runner.act(id, 'report', { terminal })`.
pub fn report(f: &Fixture, id: &str, terminal: &str) {
    f.runner
        .run
        .act(id, StageAction::Report, &from_terminal(terminal))
        .unwrap();
}

/// `runner.act(id, action, { to, reason })`, then settled.
pub fn act_to(
    f: &Fixture,
    id: &str,
    what: StageAction,
    to: Option<&str>,
    reason: Option<&str>,
) -> Task {
    let input = StageActionInput {
        to: to.map(Into::into),
        reason: reason.map(Into::into),
        ..StageActionInput::default()
    };
    let task = f.runner.run.act(id, what, &input).unwrap();
    f.settle();
    task
}

#[test]
fn writes_the_context_of_a_typed_task_and_moves_on_by_itself() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "description": "As CSV, for reports." }));

    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Ready)
    );
    let context = context_of(&f, &task.id);
    assert!(context.contains("# Export tasks"));
    assert!(context.contains("As CSV, for reports."));
    let types: Vec<String> = f
        .events(&task.id)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect();
    assert_eq!(
        types,
        [
            "task.created",
            "stage.started",
            "context.fetched",
            "stage.reported",
            "stage.checked",
            "stage.passed",
        ]
    );
}

#[test]
fn ends_the_context_of_a_subtask_with_its_parent() {
    let f = Fixture::new();
    let parent = f.created(json!({ "title": "Export", "description": "Use CSV." }));
    let task = f.created(json!({ "title": "Part A", "parentId": parent.id }));

    let context = context_of(&f, &task.id);
    assert!(context.contains("## Parent task"));
    assert!(context.contains(&format!("{}: Export", parent.id)));
    assert!(context.contains("Use CSV."));
    let folder = archive_dir(&f.data_dir, &parent.id);
    assert!(context.contains(&format!("Stage files of the parent: {}", folder.display())));
    assert!(!context_of(&f, &parent.id).contains("## Parent task"));
}

#[test]
fn reads_a_github_issue_with_its_comments_and_links_the_issue() {
    let f = Fixture::new();
    let task = f.created(
        json!({ "title": "Export", "context": { "source": "github-issue", "ref": "#7" } }),
    );

    assert_eq!(task.state, TaskState::Review);
    let issue = task.issue.unwrap();
    assert_eq!(
        (
            issue.provider.as_str(),
            issue.key.as_str(),
            issue.url.as_str()
        ),
        ("github", "#7", "https://github.com/o/r/issues/7")
    );
    assert_eq!(f.gh_calls.lock().unwrap()[0][..3], ["issue", "view", "7"]);
    assert!(context_of(&f, &task.id).contains("Dates in UTC, please."));
}

#[test]
fn runs_the_skill_that_was_picked_of_two() {
    let f = Fixture::new();
    make_skill(&f.home, "linear-issue");
    set_sources(
        &f,
        vec![
            manual_source(),
            skill_source("Jira", "jira-ticket"),
            skill_source("Linear", "linear-issue"),
        ],
    );
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "name": "Linear", "ref": "LIN-7" },
    }));

    let task = f.with_workspace(&waiting.id);
    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("/linear-issue LIN-7"));
    assert!(!script.contains("jira-ticket"));
}

#[test]
fn runs_a_skill_picked_for_the_task_with_no_source_in_the_settings() {
    let f = Fixture::new();
    make_skill(&f.home, "jira-ticket");
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" },
    }));

    let task = f.with_workspace(&waiting.id);
    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("/jira-ticket ABC-1"));
    assert!(script.contains(".natamux-task/context.md"));
    write_working(&task, "context.md", "# ABC-1\n\nExport as CSV.\n");
    report(&f, &task.id, "1");
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("context", TaskState::Review)
    );
}

#[test]
fn asks_for_the_skill_then_context_md_and_runs_the_picked_skill_over_a_namesake() {
    let f = Fixture::new();
    make_skill(&f.home, "jira-ticket");
    set_sources(&f, vec![skill_source("jira-ticket", "other-skill")]);
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" },
    }));

    let script = f.launch_script(&f.with_workspace(&waiting.id).id, "1");
    let call = script.find("/jira-ticket ABC-1").unwrap();
    let write = script
        .find("Write what you found to .natamux-task/context.md.")
        .unwrap();
    assert!(write > call);
    assert!(!script.contains("other-skill"));
    assert!(!script.contains("list them under an Attachments heading"));
}

#[test]
fn fails_the_check_of_a_picked_skill_when_the_agent_wrote_no_context_md() {
    let f = Fixture::new();
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" },
    }));
    let task = f.with_workspace(&waiting.id);
    report(&f, &task.id, "1");

    let failed = payloads_of(&f, &task.id, "stage.check-failed")
        .pop()
        .unwrap();
    assert_eq!(failed["stage"], "context");
    let reasons = failed["reasons"].as_array().unwrap();
    assert!(reasons.contains(&json!(".natamux-task/context.md is missing")));
    assert_ne!(f.task(&task.id).state, TaskState::Review);
}

#[test]
fn gives_an_agent_without_a_sure_call_the_file_of_the_picked_skill() {
    let f = Fixture::new();
    let skill = make_skill(&f.home, "jira-ticket");
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "codex",
        "context": { "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" },
    }));

    let script = f.launch_script(&f.with_workspace(&waiting.id).id, "1");
    let file = skill.join("SKILL.md");
    assert!(script.contains(&format!("Read {} and follow it for ABC-1.", file.display())));
    assert!(!script.contains("/jira-ticket ABC-1"));
}

#[test]
fn names_a_picked_skill_the_agent_does_not_have_on_the_stage() {
    let f = Fixture::new();
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "skill": "gone", "ref": "ABC-1" },
    }));

    let task = f.with_workspace(&waiting.id);
    let started = last_start(&f, &task.id, "context");
    assert_eq!(started["missingSkills"], json!(["gone"]));
    assert_eq!(started["agent"], "claude-code");
}

#[test]
fn reads_a_picked_skill_from_the_main_checkout_when_the_worktree_does_not_have_it() {
    let f = Fixture::new();
    // Not committed, so the new worktree does not have it.
    let skill = make_skill(&f.repo, "jira-ticket");
    let waiting = f.created(json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" },
    }));

    let task = f.with_workspace(&waiting.id);
    let file = skill.join("SKILL.md");
    let script = f.launch_script(&task.id, "1");
    assert!(script.contains(&format!("Read {} and follow it for ABC-1.", file.display())));
    assert!(
        last_start(&f, &task.id, "context")
            .get("missingSkills")
            .is_none()
    );
}

#[test]
fn writes_the_context_into_a_workspace_made_while_the_issue_was_read() {
    let f = Fixture::new();
    let input = json!({ "title": "Export", "context": { "source": "github-issue", "ref": "#7" } });
    let task = f.tasks.create_task(Some(&input), &[], true).unwrap();
    f.with_workspace(&task.id);

    let current = f.task(&task.id);
    assert_eq!(current.state, TaskState::Review);
    let file = Path::new(current.workspace.as_deref().unwrap()).join(".natamux-task/context.md");
    assert!(
        fs::read_to_string(file)
            .unwrap()
            .contains("Dates in UTC, please.")
    );
}

#[test]
fn reads_an_issue_of_another_repository_from_its_address() {
    let f = Fixture::new();
    let reference = "https://github.com/other/repo/issues/7#issuecomment-1";
    let task = f.created(
        json!({ "title": "Export", "context": { "source": "github-issue", "ref": reference } }),
    );

    assert_eq!(
        f.gh_calls.lock().unwrap()[0],
        [
            "issue",
            "view",
            "7",
            "--repo",
            "other/repo",
            "--json",
            "number,title,body,url,labels,comments"
        ]
    );
    assert_eq!(task.issue.unwrap().key, "other/repo#7");
}

#[test]
fn waits_for_you_when_the_issue_number_is_missing() {
    let f = Fixture::new();
    let task =
        f.created(json!({ "title": "Export", "context": { "source": "github-issue", "ref": "" } }));
    assert_eq!(task.state, TaskState::NeedsInput);
}

#[test]
fn writes_no_attachments_section_for_a_task_without_attachments() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks" }));
    assert_eq!(context_of(&f, &task.id), "# Export tasks\n\n(No notes.)\n");
}

/// Changes the stage of this id, wherever it sits in the settings (`changeStage`, `withSpec`).
pub fn change_stage(f: &Fixture, id: &str, change: impl Fn(&mut Stage)) {
    let mut settings = f.settings.lock().unwrap();
    for step in &mut settings.stages {
        match step {
            Step::Stage(stage) if stage.id == id => change(stage),
            Step::Group(group) => group
                .parallel
                .iter_mut()
                .filter(|stage| stage.id == id)
                .for_each(&change),
            Step::Branch(branch) => branch
                .paths
                .iter_mut()
                .flat_map(|path| path.stages.iter_mut())
                .filter(|stage| stage.id == id)
                .for_each(&change),
            Step::Stage(_) => {}
        }
    }
}

/// The pipeline of these stages in place of the default one.
pub fn set_stages(f: &Fixture, stages: Vec<Stage>) {
    f.settings.lock().unwrap().stages = stages.into_iter().map(Step::Stage).collect();
}

/// A `runCommand` that waits until the test ends it: `calls` are the commands and folders asked
/// for, `finish(code)` ends the command that waits.
pub struct Blocked {
    pub calls: Arc<Mutex<Vec<(String, PathBuf)>>>,
    release: Sender<i32>,
}

impl Blocked {
    pub fn new(f: &Fixture) -> Blocked {
        let (release, wait) = channel::<i32>();
        let wait = Mutex::new(wait);
        let calls: Arc<Mutex<Vec<(String, PathBuf)>>> = Arc::default();
        let seen = Arc::clone(&calls);
        f.set_run_command(move |command, cwd| {
            seen.lock()
                .unwrap()
                .push((command.into(), cwd.to_path_buf()));
            let code = wait
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap_or(-1);
            Ran {
                code,
                output: "failed".into(),
            }
        });
        Blocked { calls, release }
    }

    pub fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub fn wait_for_calls(&self, count: usize) {
        wait_for(|| self.count() >= count);
    }

    pub fn finish(&self, code: i32) {
        self.release.send(code).unwrap();
    }
}

/// `runner.act(...)` on a thread of its own: Node's promise that is awaited later.
pub fn act_in_background(
    f: &Fixture,
    id: &str,
    what: StageAction,
    input: StageActionInput,
) -> JoinHandle<Task> {
    let run = Arc::clone(&f.runner.run);
    let id = id.to_string();
    std::thread::spawn(move || run.act(&id, what, &input).unwrap())
}

/// `runner.laneFor(task, terminal)`, as (stage, state).
pub fn lane(f: &Fixture, task: &Task, terminal: &str) -> Option<(String, TaskState)> {
    f.runner.run.lane_for(task, Some(terminal)).unwrap()
}

pub fn running(stage: &str) -> Option<(String, TaskState)> {
    Some((stage.to_string(), TaskState::Running))
}

/// The payloads of `agent.launched` of an agent.
pub fn launches_of(f: &Fixture, id: &str, agent: &str) -> Vec<Value> {
    payloads_of(f, id, "agent.launched")
        .into_iter()
        .filter(|payload| payload["agent"] == agent)
        .collect()
}

pub fn any_killed(f: &Fixture) -> bool {
    f.ptys().iter().any(|pty| pty.killed())
}
