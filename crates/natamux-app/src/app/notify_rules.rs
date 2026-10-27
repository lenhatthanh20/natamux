//! Which tasks ring, what the ring says, and the count on the Dock icon (`lib/notify.ts`), and the
//! screen-reader announcement of `features/app/NeedsYouAnnouncer.tsx`. `finishedText` is
//! `workspaces/wrap_up_rules.rs`.

// Ported with their tests (spec D14); the notifier and the app shell call them.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{HashMap, HashSet};

use gpui::SystemNotification;
use natamux_proto::projects::repo::Repo;
use natamux_proto::stages::capabilities::stage_name_of_id;
use natamux_proto::tasks::task::{Task, TaskState};

use crate::tasks::task_rules::needs_me;

/// The state of each task at the last look.
pub type States = HashMap<String, TaskState>;

/// Tasks that moved into a state that needs the user since the last look. A task seen for the
/// first time does not count, so opening the app does not ring for every waiting task.
pub fn tasks_that_started_waiting<'a>(previous: &States, tasks: &'a [Task]) -> Vec<&'a Task> {
    tasks
        .iter()
        .filter(|task| {
            previous
                .get(&task.id)
                .is_some_and(|before| needs_me(task.state) && !needs_me(*before))
        })
        .collect()
}

pub fn states_by_id(tasks: &[Task]) -> States {
    tasks
        .iter()
        .map(|task| (task.id.clone(), task.state))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// The title is the task; the body says what happened at which stage, then the task id and its
/// project, for example "Spec waits for your approval" and "T-0201 · natamux".
pub fn notification_for(task: &Task, repos: &[Repo]) -> Notification {
    let stage = if task.stage.is_empty() {
        None
    } else {
        Some(stage_name_of_id(&task.stage))
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
    let what = what_happened(task.state, stage.as_deref());
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
        _ => state.label().into(),
    }
}

/// What rings after a new task list: the tasks that started to wait, then the ones that became
/// done, each with its task id, the tag that lets a newer note replace the older one.
pub fn rings(previous: &States, tasks: &[Task], repos: &[Repo]) -> Vec<(String, Notification)> {
    tasks_that_started_waiting(previous, tasks)
        .into_iter()
        .chain(tasks_that_finished(previous, tasks))
        .map(|task| (task.id.clone(), notification_for(task, repos)))
        .collect()
}

/// One sound for a batch of rings, not one for each task, and none when nothing rings.
pub fn plays_sound(ring: &[(String, Notification)]) -> bool {
    !ring.is_empty()
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
pub fn tasks_that_finished<'a>(previous: &States, tasks: &'a [Task]) -> Vec<&'a Task> {
    tasks
        .iter()
        .filter(|task| {
            previous
                .get(&task.id)
                .is_some_and(|before| *before != TaskState::Done && task.state == TaskState::Done)
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
    /// `waiting` is `needs_me_tasks`. `loaded` is false until the task list has loaded: an empty
    /// list before that is not "nothing waits".
    pub fn update(&mut self, waiting: &[Task], loaded: bool) {
        if !loaded {
            return;
        }
        let fresh: Vec<&Task> = match &self.known {
            None => Vec::new(),
            Some(known) => waiting
                .iter()
                .filter(|task| !known.contains(&task.id))
                .collect(),
        };
        self.known = Some(waiting.iter().map(|task| task.id.clone()).collect());
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
