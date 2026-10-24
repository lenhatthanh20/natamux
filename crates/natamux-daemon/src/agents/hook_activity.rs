//! What the agent in each terminal of each task does now (`activities` of
//! `agent-hook-service.ts`). In memory only: it is gone after a restart, and a new hook fills it
//! again. Each change gives back the task's whole activity, for `activity.changed`.

use natamux_agents::hooks::{NormalizedHook, Signal};
use natamux_core::js::iso_now;
use serde_json::{Map, Value, json};

/// Tasks in the order they got their first line, as a JS `Map` keeps them.
#[derive(Default)]
pub struct Activities {
    tasks: Vec<(String, Map<String, Value>)>,
}

impl Activities {
    fn lines_of(&self, task_id: &str) -> Map<String, Value> {
        self.tasks
            .iter()
            .find(|(known, _)| known == task_id)
            .map(|(_, lines)| lines.clone())
            .unwrap_or_default()
    }

    /// Stores `lines`; a task with no line is dropped. Gives the lines when `changed`.
    fn keep(
        &mut self,
        task_id: &str,
        lines: Map<String, Value>,
        changed: bool,
    ) -> Option<Map<String, Value>> {
        let position = self.tasks.iter().position(|(known, _)| known == task_id);
        match (lines.is_empty(), position) {
            (true, Some(at)) => {
                self.tasks.remove(at);
            }
            (true, None) => {}
            (false, Some(at)) => self.tasks[at].1 = lines.clone(),
            (false, None) => self.tasks.push((task_id.to_string(), lines.clone())),
        }
        changed.then_some(lines)
    }

    /// Each terminal keeps its own line, so the end of a turn in one does not clear the other.
    pub fn note(
        &mut self,
        task_id: &str,
        terminal: &str,
        hook: &NormalizedHook,
    ) -> Option<Map<String, Value>> {
        let mut lines = self.lines_of(task_id);
        let mut changed = false;
        if let Some(summary) = &hook.summary {
            lines.insert(
                terminal.to_string(),
                json!({ "text": summary, "at": iso_now() }),
            );
            changed = true;
        }
        if matches!(
            hook.signal,
            Signal::TurnEnded | Signal::Failed | Signal::Asking
        ) {
            changed = lines.shift_remove(terminal).is_some() || changed;
        }
        self.keep(task_id, lines, changed)
    }

    pub fn forget(&mut self, task_id: &str, terminal: &str) -> Option<Map<String, Value>> {
        let mut lines = self.lines_of(task_id);
        let changed = lines.shift_remove(terminal).is_some();
        self.keep(task_id, lines, changed)
    }

    /// Every line of the task is gone; tells only when there was one.
    pub fn forget_task(&mut self, task_id: &str) -> Option<Map<String, Value>> {
        let had = self.tasks.iter().any(|(known, _)| known == task_id);
        self.keep(task_id, Map::new(), had)
    }

    /// `{ taskId: { terminal: { text, at } } }`.
    pub fn all(&self) -> Map<String, Value> {
        self.tasks
            .iter()
            .map(|(task_id, lines)| (task_id.clone(), Value::Object(lines.clone())))
            .collect()
    }
}
