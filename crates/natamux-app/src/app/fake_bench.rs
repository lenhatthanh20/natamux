//! `--bench-flows` only: more tasks than the seed, and fake agents that keep the feed busy. The
//! shots never call these, so they keep the seed data.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use natamux_client::DashboardEvent;
use natamux_proto::agents::activity::AgentActivity;
use natamux_proto::tasks::task::TaskState;

use super::{Data, FakeDaemon, NOW};

/// The states of the grown tasks that are not Done, in turn.
const OPEN_STATES: [TaskState; 5] = [
    TaskState::Running,
    TaskState::NeedsInput,
    TaskState::Review,
    TaskState::Ready,
    TaskState::Blocked,
];
/// Of every 10 agent events: 3 activity, 3 usage, 2 diff stats, 1 terminals, 1 task change.
const KINDS: u64 = 10;

/// The fake agents stop when this is dropped.
pub struct FakeAgents {
    stop: Arc<AtomicBool>,
}

impl Drop for FakeAgents {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl FakeDaemon {
    /// Adds copies of the tasks, with new ids and titles, until there are `total`. The first
    /// `done` copies are Done, the others take the open states in turn. Every feed gets the list.
    pub fn grow_tasks(&self, total: usize, done: usize) {
        let mut data = self.data();
        data.grow(total, done);
        data.tasks_changed();
    }

    /// The tasks the fake has terminal rows for: the panes of the Agents grid.
    pub fn tasks_with_terminals(&self) -> Vec<String> {
        self.data().terminals.keys().cloned().collect()
    }

    /// A task changed as a hook changes it: a new title. False when there is no such task.
    pub fn touch_task(&self, task_id: &str, title: &str) -> bool {
        let mut data = self.data();
        let Some(event) = data.retitle(task_id, title) else {
            return false;
        };
        data.send(event);
        true
    }

    /// `agents` fake agents on the first tasks that are not finished send `per_second` events a
    /// second in all, on a thread of their own, until the result is dropped.
    pub fn start_agents(&'static self, agents: usize, per_second: u32) -> FakeAgents {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let gap = Duration::from_secs(1) / per_second.max(1);
        std::thread::spawn(move || {
            let mut step = 0;
            while !stopped.load(Ordering::Relaxed) {
                self.agent_step(step, agents);
                step += 1;
                std::thread::sleep(gap);
            }
        });
        FakeAgents { stop }
    }

    /// One event of agent `step % agents`, to every feed.
    pub fn agent_step(&self, step: u64, agents: usize) {
        let mut data = self.data();
        if let Some(event) = data.agent_event(step, agents) {
            data.send(event);
        }
    }
}

impl Data {
    fn grow(&mut self, total: usize, done: usize) {
        let originals = self.tasks.clone();
        let mut added = 0;
        while self.tasks.len() < total && !originals.is_empty() {
            let mut task = originals[added % originals.len()].clone();
            self.next_number += 1;
            task.id = format!("T-{:04}", self.next_number);
            task.title = format!("Bench copy {} of {}", self.next_number, task.title);
            // A copy is no subtask and waits for nothing: the seed's links stay the seed's.
            task.parent_id = None;
            task.depends_on.clear();
            task.state = match added.checked_sub(done) {
                None => TaskState::Done,
                Some(open) => OPEN_STATES[open % OPEN_STATES.len()],
            };
            self.tasks.push(task);
            added += 1;
        }
    }

    /// The new title, as the one-task event the client sends after the change.
    fn retitle(&mut self, task_id: &str, title: &str) -> Option<DashboardEvent> {
        let task = self.task_mut(task_id).ok()?;
        task.title = title.into();
        task.updated_at = NOW.into();
        Some(DashboardEvent::Tasks(self.tasks.clone()))
    }

    fn agent_event(&mut self, step: u64, agents: usize) -> Option<DashboardEvent> {
        let ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|task| !matches!(task.state, TaskState::Done | TaskState::Cancelled))
            .take(agents)
            .map(|task| task.id.clone())
            .collect();
        let index = usize::try_from(step).ok()? % ids.len().max(1);
        let id = ids.get(index)?.clone();
        Some(match step % KINDS {
            0..=2 => {
                let line = AgentActivity {
                    text: format!("Edit src/bench_{step}.rs"),
                    at: NOW.into(),
                };
                let lines = self.activity.entry(id).or_default();
                lines.insert("1".into(), line);
                DashboardEvent::Activity(self.activity.clone())
            }
            3..=5 => {
                let usage = self.usage.entry(id).or_default();
                usage.tokens += 1_000;
                usage.cost_usd = Some(usage.tokens as f64 * 0.000_003);
                DashboardEvent::Usage(self.usage.clone())
            }
            6 | 7 => {
                let stat = self.diff_stats.entry(id).or_default();
                stat.files = 1 + step % 7;
                stat.added += 3;
                stat.removed += 1;
                DashboardEvent::DiffStats(self.diff_stats.clone())
            }
            8 => {
                let list = self.terminals.entry(id.clone()).or_default();
                if !list.terminals.iter().any(|name| name == "1") {
                    list.terminals.push("1".into());
                }
                let program = if step.is_multiple_of(2) {
                    "claude"
                } else {
                    "zsh"
                };
                list.programs.insert("1".into(), Some(program.into()));
                DashboardEvent::Terminals {
                    task_id: id,
                    terminals: list.clone(),
                }
            }
            _ => self.retitle(&id, &format!("Bench task changed at step {step}"))?,
        })
    }
}
