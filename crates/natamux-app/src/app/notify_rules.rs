//! Which tasks ring, what the ring says, and the count on the Dock icon (`lib/notify.ts`), and the
//! screen-reader announcement of `features/app/NeedsYouAnnouncer.tsx`. `finishedText` is
//! `workspaces/wrap_up_rules.rs`.

// Ported with their tests (spec D14); the notifier and the app shell call them.
#![cfg_attr(not(test), allow(dead_code))]

use std::borrow::Borrow;
use std::collections::{BTreeSet, HashMap, HashSet};

use gpui::SystemNotification;
use natamux_proto::projects::repo::Repo;
use natamux_proto::stages::capabilities::stage_name_of_id;
use natamux_proto::tasks::task::{Task, TaskState};

use crate::app::view_rules::is_unseen;
use crate::app::view_rules::seen_results::is_result_unseen;
use crate::tasks::task_changes::task_of;
use crate::tasks::task_rules::needs_me;

/// What the last look saw of a task: its state, the stages whose lane waited for the user, and
/// the stages whose lane was idle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    pub state: TaskState,
    pub waits: BTreeSet<String>,
    pub idle: BTreeSet<String>,
    /// The task's last background result (`backgroundResultAt`, T-0390).
    pub result: Option<String>,
}

/// Each task at the last look.
pub type States = HashMap<String, Seen>;

const KNOWN_STATES: [TaskState; 10] = [
    TaskState::Draft,
    TaskState::Ready,
    TaskState::Running,
    TaskState::Idle,
    TaskState::NeedsInput,
    TaskState::Review,
    TaskState::Blocked,
    TaskState::Failed,
    TaskState::Done,
    TaskState::Cancelled,
];

/// The state a lane holds, read without cloning the value: as serde reads it, a name this build
/// does not know is `Unknown`, and a value that is no string is none.
fn lane_state(lane: &serde_json::Value) -> Option<TaskState> {
    let name = lane.as_str()?;
    Some(
        KNOWN_STATES
            .into_iter()
            .find(|state| state.name() == name)
            .unwrap_or(TaskState::Unknown),
    )
}

/// The stages whose lane is in a state `keep` takes.
fn stages_where(task: &Task, keep: impl Fn(TaskState) -> bool) -> BTreeSet<String> {
    task.lanes
        .iter()
        .filter(|(_, state)| lane_state(state).is_some_and(&keep))
        .map(|(stage, _)| stage.clone())
        .collect()
}

/// The stages whose lane needs the user now. Stages run side by side, each in its own terminal,
/// so one can start to wait while the task already waits at another.
fn waiting_stages(task: &Task) -> BTreeSet<String> {
    stages_where(task, needs_me)
}

/// The stages whose agent ended its turn without a question and without `natamux done`.
fn idle_stages(task: &Task) -> BTreeSet<String> {
    stages_where(task, |state| state == TaskState::Idle)
}

/// Tasks that moved into a state that needs the user since the last look. A task seen for the
/// first time does not count, so opening the app does not ring for every waiting task.
pub fn tasks_that_started_waiting<'a, T: Borrow<Task>>(
    previous: &States,
    tasks: &'a [T],
) -> Vec<&'a Task> {
    tasks
        .iter()
        .map(task_of)
        .filter(|task| {
            previous
                .get(&task.id)
                .is_some_and(|before| needs_me(task.state) && !needs_me(before.state))
        })
        .collect()
}

/// What a look sees of one task.
fn seen_of(task: &Task) -> Seen {
    Seen {
        state: task.state,
        waits: waiting_stages(task),
        idle: idle_stages(task),
        result: task.background_result_at.clone(),
    }
}

pub fn states_by_id<T: Borrow<Task>>(tasks: &[T]) -> States {
    tasks
        .iter()
        .map(task_of)
        .map(|task| (task.id.clone(), seen_of(task)))
        .collect()
}

/// A look at the one task that changed since the last look: `previous` then holds the task as it
/// is now, with no look at the other tasks. What comes back holds only how the task was, so
/// `rings` of it and this one task gives what `rings` of both whole lists would.
pub fn look_at_one(previous: &mut States, task: &Task) -> States {
    let now = seen_of(task);
    match previous.get_mut(&task.id) {
        Some(held) => States::from([(task.id.clone(), std::mem::replace(held, now))]),
        None => {
            previous.insert(task.id.clone(), now);
            States::new()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// The title is the task; the body says what happened at which stage, then the task id and its
/// project, for example "Spec waits for your approval" and "T-0201 · natamux".
pub fn notification_for(task: &Task, repos: &[Repo]) -> Notification {
    notification_at(task, task.state, &task.stage, repos)
}

/// The note of one stage of the task, in the state of its lane.
fn notification_at(task: &Task, state: TaskState, stage: &str, repos: &[Repo]) -> Notification {
    note_of(task, state, stage, false, repos)
}

/// The note of a stage; `result` names a background result instead of the state.
fn note_of(
    task: &Task,
    state: TaskState,
    stage: &str,
    result: bool,
    repos: &[Repo],
) -> Notification {
    let stage = if stage.is_empty() {
        None
    } else {
        Some(stage_name_of_id(stage))
    };
    let project = task
        .repo
        .as_deref()
        .and_then(|id| repos.iter().find(|repo| repo.id == id));
    let place = match project {
        Some(repo) => format!("{} · {}", task.id, repo.name),
        None => task.id.clone(),
    };
    // The body is two lines: a line break in a name would push the place onto more.
    let place = place.replace(['\r', '\n'], " ");
    let what = match (result, stage.as_deref()) {
        (true, Some(stage)) => format!("New background result at {stage}"),
        (true, None) => "New background result".into(),
        (false, stage) => what_happened(state, stage),
    };
    Notification {
        title: task.title.clone(),
        body: format!("{what}\n{place}"),
    }
}

fn what_happened(state: TaskState, stage: Option<&str>) -> String {
    match (state, stage) {
        (TaskState::NeedsInput, Some(stage)) => format!("Needs your input at {stage}"),
        (TaskState::NeedsInput, None) => "Needs your input".into(),
        (TaskState::Review, Some(stage)) => format!("{stage} waits for your approval"),
        (TaskState::Review, None) => "Waits for your review".into(),
        (TaskState::Failed | TaskState::Blocked, Some(stage)) => {
            format!("{} at {stage}", state.label())
        }
        (TaskState::Idle, Some(stage)) => format!("Finished its turn at {stage}"),
        (TaskState::Idle, None) => "Finished its turn".into(),
        _ => state.label().into(),
    }
}

/// One note to post. `stage` is the stage whose lane waits, so the note can name its terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ring {
    pub task_id: String,
    pub stage: Option<String>,
    pub note: Notification,
}

/// The stages that became idle since the last look: an agent ended its turn with a report, which
/// the user may want to read. A task with no lanes counts with its own stage.
fn newly_idle(before: &Seen, task: &Task) -> Vec<(String, TaskState)> {
    let mut idle: Vec<(String, TaskState)> = idle_stages(task)
        .difference(&before.idle)
        .map(|stage| (stage.clone(), TaskState::Idle))
        .collect();
    let task_idle = task.state == TaskState::Idle && before.state != TaskState::Idle;
    if idle.is_empty() && task_idle && task.lanes.is_empty() {
        idle.push((task.stage.clone(), TaskState::Idle));
    }
    idle
}

/// A new background result since the last look (T-0390).
fn new_result(before: &Seen, task: &Task) -> bool {
    task.background_result_at.is_some() && task.background_result_at != before.result
}

/// What rings after a new task list: the stages that started to wait (a task that waits already
/// rings again when another of its terminals starts to wait), the stages whose agent ended its
/// turn, then the tasks that became done.
pub fn rings<T: Borrow<Task>>(previous: &States, tasks: &[T], repos: &[Repo]) -> Vec<Ring> {
    rings_with(previous, tasks, repos, false)
}

/// `rings`, where a turn that background work started rings only when `results` is on: a loop
/// may end such a turn every few minutes. The user's own turn ends ring as before.
pub fn rings_with<T: Borrow<Task>>(
    previous: &States,
    tasks: &[T],
    repos: &[Repo],
    results: bool,
) -> Vec<Ring> {
    let started: HashSet<&str> = tasks_that_started_waiting(previous, tasks)
        .iter()
        .map(|task| task.id.as_str())
        .collect();
    let mut ring = Vec::new();
    for task in tasks.iter().map(task_of) {
        let Some(before) = previous.get(&task.id) else {
            continue;
        };
        let mut new: Vec<(String, TaskState)> = waiting_stages(task)
            .difference(&before.waits)
            .filter_map(|stage| {
                let state = lane_state(task.lanes.get(stage)?)?;
                Some((stage.clone(), state))
            })
            .collect();
        if new.is_empty() && started.contains(task.id.as_str()) {
            new.push((task.stage.clone(), task.state));
        }
        let result = new_result(before, task);
        if !result {
            new.extend(newly_idle(before, task));
        }
        if result && results {
            ring.push(Ring {
                task_id: task.id.clone(),
                note: note_of(task, task.state, &task.stage, true, repos),
                stage: (!task.stage.is_empty()).then(|| task.stage.clone()),
            });
        }
        for (stage, state) in new {
            ring.push(Ring {
                task_id: task.id.clone(),
                note: notification_at(task, state, &stage, repos),
                stage: (!stage.is_empty()).then_some(stage),
            });
        }
    }
    ring.extend(
        tasks_that_finished(previous, tasks)
            .into_iter()
            .map(|task| Ring {
                task_id: task.id.clone(),
                stage: None,
                note: notification_for(task, repos),
            }),
    );
    ring
}

/// The note names the terminal that waits: "Needs your input at Self check · Terminal 17".
pub fn with_terminal(note: &Notification, terminal: &str) -> Notification {
    let body = match note.body.split_once('\n') {
        Some((what, place)) => format!("{what} \u{b7} Terminal {terminal}\n{place}"),
        None => format!("{} \u{b7} Terminal {terminal}", note.body),
    };
    Notification {
        title: note.title.clone(),
        body,
    }
}

/// The tag of a note: the task id, and the terminal after a `#` when the note names one. Task ids
/// hold no `#` (`T-42`), so the click can split it back.
pub fn note_tag(task_id: &str, terminal: Option<&str>) -> String {
    match terminal {
        Some(terminal) => format!("{task_id}#{terminal}"),
        None => task_id.to_string(),
    }
}

/// The task and the terminal a clicked note names.
pub fn tag_target(tag: &str) -> (&str, Option<&str>) {
    match tag.split_once('#') {
        Some((task_id, terminal)) => (task_id, Some(terminal)),
        None => (tag, None),
    }
}

/// One sound for a batch of rings, not one for each task, and none when nothing rings.
pub fn plays_sound<T>(ring: &[T]) -> bool {
    !ring.is_empty()
}

/// Idle tasks the user has not looked at since their agent ended its turn: the report still
/// waits to be read, so the Dock counts them until the task is opened.
pub fn unread_turn_ends<T: Borrow<Task>>(tasks: &[T], seen: &[(String, TaskState)]) -> usize {
    tasks
        .iter()
        .map(task_of)
        .filter(|task| task.state == TaskState::Idle && is_unseen(seen, &task.id, task.state))
        .count()
}

/// Tasks with news the user has not looked at: an idle task whose turn end is unseen, or a
/// background result not read yet (T-0390). Each task counts once.
pub fn unread_count<T: Borrow<Task>>(
    tasks: &[T],
    seen: &[(String, TaskState)],
    results: &[(String, String)],
) -> usize {
    tasks
        .iter()
        .map(task_of)
        .filter(|task| {
            (task.state == TaskState::Idle && is_unseen(seen, &task.id, task.state))
                || is_result_unseen(results, task)
        })
        .count()
}

/// The label of the Dock icon: the needs-you count, none when nothing waits.
pub fn badge_label(needs_you: usize) -> Option<String> {
    (needs_you > 0).then(|| needs_you.to_string())
}

/// Posted when the user turns notifications on: macOS asks for permission on the first note, so
/// it asks now, while the user looks, and not when the first task rings with the app in the back.
pub fn turned_on() -> SystemNotification {
    SystemNotification {
        tag: "notifications-on".into(),
        title: "Notifications are on".into(),
        body: "Natamux tells you here when a task needs you.".into(),
        actions: Vec::new(),
    }
}

/// Tasks that became done since the last look. A cancel is always someone's own act, so it does
/// not ring.
pub fn tasks_that_finished<'a, T: Borrow<Task>>(
    previous: &States,
    tasks: &'a [T],
) -> Vec<&'a Task> {
    tasks
        .iter()
        .map(task_of)
        .filter(|task| {
            previous.get(&task.id).is_some_and(|before| {
                before.state != TaskState::Done && task.state == TaskState::Done
            })
        })
        .collect()
}

/// What a screen reader says, politely, when a task starts to wait for the user.
#[derive(Debug, Default)]
pub struct NeedsYouAnnouncer {
    /// The tasks that waited already. None before the first loaded update: those are not news.
    known: Option<HashSet<String>>,
    announcement: String,
}

impl NeedsYouAnnouncer {
    /// `waiting` is `needs_me_refs`. `loaded` is false until the task list has loaded: an empty
    /// list before that is not "nothing waits".
    pub fn update<T: Borrow<Task>>(&mut self, waiting: &[T], loaded: bool) {
        if !loaded {
            return;
        }
        let fresh: Vec<&Task> = match &self.known {
            None => Vec::new(),
            Some(known) => waiting
                .iter()
                .map(task_of)
                .filter(|task| !known.contains(&task.id))
                .collect(),
        };
        let ids = waiting.iter().map(|task| task_of(task).id.clone());
        self.known = Some(ids.collect());
        if !fresh.is_empty() {
            let lines: Vec<String> = fresh
                .iter()
                .map(|task| format!("{} needs you", task.title))
                .collect();
            self.announcement = lines.join(". ");
        }
    }

    /// The text of the last announcement; it stays until the next one.
    pub fn text(&self) -> &str {
        &self.announcement
    }
}

#[cfg(test)]
#[path = "notify_rules_tests.rs"]
mod tests;
