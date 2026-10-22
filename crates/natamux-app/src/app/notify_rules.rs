//! Which tasks ring, and what the ring says (`lib/notify.ts`), and the screen-reader announcement
//! of `features/app/NeedsYouAnnouncer.tsx`. `finishedText` is `workspaces/wrap_up_rules.rs`.

// Ported with their tests (spec D14); the notifier and the app shell call them.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{HashMap, HashSet};

use natamux_proto::tasks::task::{Task, TaskState};

use crate::tasks::task_rules::{needs_me, state_label};

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

pub fn notification_for(task: &Task) -> Notification {
    Notification {
        title: format!("{}: {}", task.id, state_label(task.state)),
        body: task.title.clone(),
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
