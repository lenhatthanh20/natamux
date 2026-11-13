//! Recover the current wait from the event log, including waits raised by a terminal bell.

use natamux_agents::hooks::Wait;
use natamux_core::events::moves::move_of;
use natamux_proto::events::task_event::StoredEvent;
use serde_json::Value;

pub(super) fn wait_of(events: &[StoredEvent], stage: &str, step: &str, terminal: &str) -> Wait {
    let mut resumed_session = None;
    for (index, stored) in events.iter().enumerate().rev() {
        let event = &stored.event;
        let text = |key| event.payload.get(key).and_then(Value::as_str);
        // Reopening or overriding a step can invalidate all of its old lane waits.
        let resets_step = matches!(
            event.kind.as_str(),
            "stage.reopened" | "review.changes-requested" | "task.cancelled"
        );
        let overrides_lane = event.kind == "state.overridden"
            && text("stage").is_none_or(|named| named == stage || named == step);
        if stored.accepted && (resets_step || overrides_lane) {
            return Wait::Answer;
        }
        let belongs = match text("stage") {
            Some(named) => named == stage,
            None => text("terminal").is_none_or(|named| named == terminal),
        };
        if !belongs {
            continue;
        }
        if stored.accepted && event.kind == "agent.launched" {
            // Restoring a named session keeps its pending permission. A fresh or unknown
            // session must not inherit a wait from the previous run in this terminal.
            let Some(session) = text("resumeSessionId").filter(|id| !id.is_empty()) else {
                return Wait::Answer;
            };
            if resumed_session.is_some_and(|known| known != session) {
                return Wait::Answer;
            }
            resumed_session = Some(session);
            continue;
        }
        if event.kind == "agent.asked" {
            // A second explicit question is logged even while the lane already needs input.
            // It replaces a permission wait, although it cannot move the state again.
            let hook_wait = event.source == "agent"
                && (text("stage").is_some() || text("terminal").is_some())
                && (text("reason") == Some("terminal bell")
                    || matches!(
                        text("hook"),
                        Some("PermissionRequest" | "Notification" | "notification" | "PreToolUse")
                    ));
            if !hook_wait {
                return Wait::Answer;
            }
            if stored.accepted {
                if let (Some(resumed), Some(previous)) = (
                    resumed_session,
                    session_of_wait(&events[..=index], stage, terminal),
                ) && resumed != previous
                {
                    return Wait::Answer;
                }
                return Wait::Hook;
            }
        } else if stored.accepted && move_of(event).is_some() {
            return Wait::Answer;
        }
    }
    Wait::Answer
}

fn session_of_wait<'a>(events: &'a [StoredEvent], stage: &str, terminal: &str) -> Option<&'a str> {
    for stored in events.iter().rev().filter(|stored| stored.accepted) {
        let event = &stored.event;
        let text = |key| event.payload.get(key).and_then(Value::as_str);
        let belongs = match text("stage") {
            Some(named) => named == stage,
            None => text("terminal").is_none_or(|named| named == terminal),
        };
        if !belongs {
            continue;
        }
        if event.source == "agent"
            && let Some(session) = text("sessionId")
        {
            return Some(session);
        }
        if (event.kind != "agent.asked" && move_of(event).is_some())
            || event.kind == "agent.launched"
        {
            return None;
        }
    }
    None
}

#[cfg(test)]
#[path = "hook_waits_tests.rs"]
mod tests;
