//! Which turns of an agent background work started, not the user (T-0390). A turn that ends with
//! background work left is woken again by it; the turn it wakes for ends with news the user may
//! not have seen. Kept in memory only: after a restart of the daemon no turn is marked.

use std::collections::HashMap;

use natamux_agents::hooks::{
    BACKGROUND, BACKGROUND_RESULT_KEY, NormalizedHook, PROMPT_HEAD_KEY, SCHEDULED_KEY, Signal,
    WAKE_KEY,
};
use serde_json::Value;

#[derive(Default)]
struct Turn {
    /// The user sent a prompt since the last turn end.
    prompted: bool,
    /// The last turn end left background work or scheduled prompts.
    left_work: bool,
    /// The scheduled prompts that turn end named, to tell their prompt from one of the user.
    scheduled: Vec<String>,
}

/// The turns of each shell, by "<task>/<terminal>".
#[derive(Default)]
pub(super) struct Turns(HashMap<String, Turn>);

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

impl Turns {
    /// Notes a hook of a shell. A turn end of a turn that background work started, which leaves
    /// work again, gets `backgroundResult`. The start of a prompt leaves the details here, so it
    /// is never logged. Gives the signal for the idle pause: a turn end with scheduled prompts left
    /// counts as paused, or the pause would end the shell and the prompts with it.
    pub(super) fn note(
        &mut self,
        task_id: &str,
        terminal: &str,
        hook: &mut NormalizedHook,
    ) -> Signal {
        let head = hook.details.remove(PROMPT_HEAD_KEY);
        let key = format!("{task_id}/{terminal}");
        match hook.signal {
            Signal::Prompt => {
                let turn = self.0.entry(key).or_default();
                let sent_by_work =
                    hook.details.get(WAKE_KEY).and_then(Value::as_str) == Some(BACKGROUND);
                let scheduled = head
                    .as_ref()
                    .and_then(Value::as_str)
                    .is_some_and(|head| turn.scheduled.iter().any(|prompt| prompt == head));
                if !sent_by_work && !scheduled {
                    turn.prompted = true;
                }
                hook.signal
            }
            Signal::TurnEnded | Signal::TurnPaused | Signal::TurnAsked => {
                let scheduled = strings(hook.details.get(SCHEDULED_KEY));
                let leaves_work = hook.signal == Signal::TurnPaused || !scheduled.is_empty();
                let turn = self.0.entry(key).or_default();
                // A question rings on its own; a last turn with nothing left ends as usual.
                if turn.left_work
                    && !turn.prompted
                    && leaves_work
                    && hook.signal != Signal::TurnAsked
                {
                    hook.details
                        .insert(BACKGROUND_RESULT_KEY.into(), Value::Bool(true));
                }
                *turn = Turn {
                    prompted: false,
                    left_work: leaves_work,
                    scheduled,
                };
                if hook.signal == Signal::TurnEnded && leaves_work {
                    Signal::TurnPaused
                } else {
                    hook.signal
                }
            }
            Signal::Failed => {
                self.0.remove(&key);
                hook.signal
            }
            Signal::Activity | Signal::Asking => hook.signal,
        }
    }

    /// The shell ended: the next agent there starts with no turn behind it.
    pub(super) fn forget(&mut self, task_id: &str, terminal: &str) {
        self.0.remove(&format!("{task_id}/{terminal}"));
    }
}

#[cfg(test)]
#[path = "hook_turns_tests.rs"]
mod tests;
