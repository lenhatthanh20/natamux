//! Recover the current wait from the event log, including waits raised by a terminal bell.

use natamux_agents::hooks::Wait;
use natamux_core::events::moves::move_of;
use natamux_proto::events::task_event::StoredEvent;
use serde_json::Value;

/// How many events the first read of the log's end asks for. The wait of a lane is nearly always
/// in the last few events; the whole log of a long task has thousands.
const TAIL: usize = 64;

/// The current wait of a lane, read from the end of the task's events: `tail(limit)` gives the
/// last `limit` of them, oldest first. Reads further back only when those do not decide it.
pub(super) fn wait_at_end<E>(
    mut tail: impl FnMut(usize) -> Result<Vec<StoredEvent>, E>,
    stage: &str,
    step: &str,
    terminal: &str,
) -> Result<Wait, E> {
    let mut limit = TAIL;
    loop {
        let events = tail(limit)?;
        match wait_in(&events, stage, step, terminal) {
            Ok(wait) => return Ok(wait),
            // Fewer events than asked for: this is the whole log.
            Err(whole) if events.len() < limit => return Ok(whole),
            Err(_) => limit = limit.saturating_mul(8),
        }
    }
}

/// The wait when `events`, the end of a log, decide it. Else, as an error, the wait in case
/// they are the whole log: an older event may still change it.
fn wait_in(events: &[StoredEvent], stage: &str, step: &str, terminal: &str) -> Result<Wait, Wait> {
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
            return Ok(Wait::Answer);
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
                return Ok(Wait::Answer);
            };
            if resumed_session.is_some_and(|known| known != session) {
                return Ok(Wait::Answer);
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
                return Ok(Wait::Answer);
            }
            if stored.accepted {
                let Some(resumed) = resumed_session else {
                    return Ok(Wait::Hook);
                };
                return match session_of_wait(&events[..=index], stage, terminal) {
                    Session::Named(previous) if previous != resumed => Ok(Wait::Answer),
                    Session::Older => Err(Wait::Hook),
                    _ => Ok(Wait::Hook),
                };
            }
        } else if stored.accepted && move_of(event).is_some() {
            return Ok(Wait::Answer);
        }
    }
    Err(Wait::Answer)
}

/// The session of a wait, as far as the events given tell.
enum Session<'a> {
    Named(&'a str),
    Unknown,
    /// No event given tells: an older one may.
    Older,
}

fn session_of_wait<'a>(events: &'a [StoredEvent], stage: &str, terminal: &str) -> Session<'a> {
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
            return Session::Named(session);
        }
        if (event.kind != "agent.asked" && move_of(event).is_some())
            || event.kind == "agent.launched"
        {
            return Session::Unknown;
        }
    }
    Session::Older
}

#[cfg(test)]
#[path = "hook_waits_tests.rs"]
mod tests;
