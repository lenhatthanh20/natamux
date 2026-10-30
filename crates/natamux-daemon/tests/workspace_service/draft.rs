//! `describe('draftCommitMessage')`. The agent run is a stub: it records each `AgentRun` and
//! answers with what the test last set. Node's two asks "at the same time" are two threads.

use std::collections::HashSet;
use std::fs;
use std::sync::mpsc::Sender;
use std::time::Duration;

use natamux_git::commit_drafter::AgentRun;
use natamux_proto::workspaces::changes::{CommitMessageDraft, DraftSource};

use super::*;

type Answer = Arc<dyn Fn(&AgentRun) -> Result<String, String> + Send + Sync>;

struct Drafts {
    ws: Arc<WorkspaceService>,
    runs: Arc<Mutex<Vec<AgentRun>>>,
    answer: Arc<Mutex<Answer>>,
}

impl Drafts {
    fn new(fx: &Fx) -> Drafts {
        let runs: Arc<Mutex<Vec<AgentRun>>> = Arc::default();
        let first: Answer = Arc::new(|_| Ok("Add the fix\n".to_string()));
        let answer = Arc::new(Mutex::new(first));
        let (seen, pick) = (Arc::clone(&runs), Arc::clone(&answer));
        let ws = fx.build(|options| {
            options.program_of = Arc::new(|agent| format!("my-{agent}"));
            options.run_agent_once = Arc::new(move |run| {
                seen.lock().unwrap().push(run.clone());
                let answer = Arc::clone(&pick.lock().unwrap());
                answer(&run)
            });
        });
        Drafts { ws, runs, answer }
    }

    fn answer_with(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }

    fn run_count(&self) -> usize {
        self.runs.lock().unwrap().len()
    }

    fn draft(&self, task_id: &str) -> CommitMessageDraft {
        self.ws.draft_commit_message(task_id).unwrap()
    }

    // A task with a workspace and a new file.
    fn task_with_work(&self, fx: &Fx) -> Task {
        let task = fx.task("Fix the bug");
        let created = self.ws.create(&task.id, None, None).unwrap();
        fs::write(
            PathBuf::from(created.workspace.clone().unwrap()).join("fix.txt"),
            "fixed\n",
        )
        .unwrap();
        // Only the drafts count here, not the times of the new workspace.
        fx.log.lock().unwrap().clear();
        created
    }

    fn ask_in_thread(&self, task_id: &str) -> std::thread::JoinHandle<CommitMessageDraft> {
        let (ws, id) = (Arc::clone(&self.ws), task_id.to_string());
        std::thread::spawn(move || ws.draft_commit_message(&id).unwrap())
    }
}

/// An answer that waits for one send on the returned sender.
fn held_answer() -> (Answer, Sender<()>) {
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = Mutex::new(wait);
    let answer: Answer = Arc::new(move |_| {
        let _ = wait.lock().unwrap().recv_timeout(Duration::from_secs(30));
        Ok("Add the fix".to_string())
    });
    (answer, release)
}

fn agent_draft(message: &str, agent: &str) -> CommitMessageDraft {
    CommitMessageDraft {
        message: message.into(),
        source: DraftSource::Agent,
        agent: agent.into(),
        reason: None,
    }
}

#[test]
fn asks_the_task_agent_with_the_diff_and_leaves_the_worktree_as_it_was() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = drafts.task_with_work(&fx);
    fx.tasks
        .record_agent_launch(&task.id, "codex", None, None, None)
        .unwrap();
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    let status = run_git(&workspace, &["status", "--porcelain"]);
    let events = fx.events(&task.id).len();

    let draft = drafts.draft(&task.id);

    assert_eq!(draft, agent_draft("Add the fix", "codex"));
    let runs = drafts.runs.lock().unwrap().clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].agent, "codex");
    assert_eq!(runs[0].program, "my-codex");
    assert_eq!(runs[0].cwd, workspace);
    assert!(runs[0].prompt.contains("Task: Fix the bug"));
    assert!(runs[0].prompt.contains("+fixed"));
    assert_eq!(run_git(&workspace, &["status", "--porcelain"]), status);
    assert_eq!(fx.events(&task.id).len(), events);
    assert_eq!(fx.state_of(&task.id), task.state);
}

#[test]
fn uses_claude_code_for_a_task_whose_agent_is_null() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = drafts.task_with_work(&fx);

    assert_eq!(drafts.draft(&task.id).agent, "claude-code");
    assert_eq!(drafts.runs.lock().unwrap()[0].program, "my-claude-code");
}

#[test]
fn uses_claude_code_for_a_task_whose_agent_is_shell() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = drafts.task_with_work(&fx);
    fx.tasks
        .record_agent_launch(&task.id, "shell", None, None, None)
        .unwrap();

    assert_eq!(drafts.draft(&task.id).agent, "claude-code");
    assert_eq!(drafts.runs.lock().unwrap()[0].program, "my-claude-code");
}

#[test]
fn gives_the_simple_draft_when_the_agent_fails() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = drafts.task_with_work(&fx);
    let why = "my-claude-code exited with code 127";
    drafts.answer_with(Arc::new(move |_| Err(why.to_string())));

    let expected = CommitMessageDraft {
        message: "Add fix.txt".into(),
        source: DraftSource::Draft,
        agent: "claude-code".into(),
        reason: Some(why.into()),
    };
    assert_eq!(drafts.draft(&task.id), expected);
    let line = format!(
        "No commit message draft by claude-code for {}: {why}",
        task.id
    );
    assert_eq!(fx.log_lines(), [line]);
}

#[test]
fn logs_nothing_when_the_agent_drafts() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = drafts.task_with_work(&fx);
    drafts.draft(&task.id);
    assert!(fx.log_lines().is_empty());
}

// The second ask comes while the first run is held. A run that began for it would show in the
// count; the short wait gives the second thread time to reach the shared run.
#[test]
fn runs_the_agent_once_for_two_asks_at_the_same_time_and_again_after() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = drafts.task_with_work(&fx);
    let (answer, release) = held_answer();
    drafts.answer_with(answer);

    let first = drafts.ask_in_thread(&task.id);
    wait_for("the first run", || drafts.run_count() == 1);
    let second = drafts.ask_in_thread(&task.id);
    std::thread::sleep(Duration::from_millis(300));
    release.send(()).unwrap();
    let _ = release.send(());
    let (first, second) = (first.join().unwrap(), second.join().unwrap());
    assert_eq!(second, first);
    assert_eq!(drafts.run_count(), 1);

    drafts.answer_with(Arc::new(|_| Ok("Add the fix".to_string())));
    drafts.draft(&task.id);
    assert_eq!(drafts.run_count(), 2);
}

#[test]
fn starts_a_new_run_after_a_failed_one_and_runs_once_per_task() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let one = drafts.task_with_work(&fx);
    drafts.answer_with(Arc::new(|_| Err("boom".to_string())));
    assert_eq!(drafts.draft(&one.id).source, DraftSource::Draft);
    drafts.answer_with(Arc::new(|_| Ok("Add the fix".to_string())));
    assert_eq!(drafts.draft(&one.id).source, DraftSource::Agent);
    assert_eq!(drafts.run_count(), 2);

    let other = fx.task("Other");
    let created = drafts.ws.create(&other.id, None, None).unwrap();
    let other_dir = PathBuf::from(created.workspace.clone().unwrap());
    fs::write(other_dir.join("other.txt"), "x\n").unwrap();
    let asks = [
        drafts.ask_in_thread(&one.id),
        drafts.ask_in_thread(&other.id),
    ];
    for ask in asks {
        ask.join().unwrap();
    }
    let runs = drafts.runs.lock().unwrap().clone();
    let cwds: HashSet<PathBuf> = runs[2..].iter().map(|run| run.cwd.clone()).collect();
    let expected = HashSet::from([PathBuf::from(one.workspace.clone().unwrap()), other_dir]);
    assert_eq!(cwds, expected);
}

#[test]
fn does_not_wait_for_a_push_and_does_not_hold_a_commit_back() {
    let fx = Fx::new();
    let (gh, release_gh) = blocked_gh();
    let first = Held::default();
    let service = fx.build(|options| {
        options.run_gh = gh;
        options.run_agent_once = first.run();
    });
    bare_origin(fx.root.path());
    let task = fx.task("Fix the bug");
    let created = service.create(&task.id, None, None).unwrap();
    let workspace = PathBuf::from(created.workspace.clone().unwrap());
    fs::write(workspace.join("fix.txt"), "fixed\n").unwrap();
    service.commit(&task.id, "First").unwrap();
    fs::write(workspace.join("more.txt"), "more\n").unwrap();

    let (opener, id) = (Arc::clone(&service), task.id.clone());
    let opening = std::thread::spawn(move || opener.open_pull_request(&id));
    let (drafter, id) = (Arc::clone(&service), task.id.clone());
    let drafting = std::thread::spawn(move || drafter.draft_commit_message(&id));
    first.wait_started();
    first.finish("Add more");
    assert_eq!(drafting.join().unwrap().unwrap().message, "Add more");
    release_gh.send(()).unwrap();
    opening.join().unwrap().unwrap();

    let second = Held::default();
    let later = fx.build(|options| options.run_agent_once = second.run());
    let (drafter, id) = (Arc::clone(&later), task.id.clone());
    let waiting = std::thread::spawn(move || drafter.draft_commit_message(&id));
    second.wait_started();
    later.commit(&task.id, "Add more").unwrap();
    second.finish("Late");
    assert_eq!(waiting.join().unwrap().unwrap().message, "Late");
}

#[test]
fn refuses_when_nothing_changed_or_there_is_no_workspace() {
    let fx = Fx::new();
    let drafts = Drafts::new(&fx);
    let task = fx.task("Fix the bug");
    assert!(is_conflict(&drafts.ws.draft_commit_message(&task.id)));
    drafts.ws.create(&task.id, None, None).unwrap();
    let result = drafts.ws.draft_commit_message(&task.id);
    assert!(matches!(result, Err(WorkspaceError::NothingToCommit)));
    assert_eq!(drafts.run_count(), 0);
}
