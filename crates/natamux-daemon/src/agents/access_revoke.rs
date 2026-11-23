//! A finished task's agent token stops working (F6). After Done or Cancelled a grace lets the
//! agent's last hooks and `natamux done` arrive, then the token goes. A reopen before then keeps
//! it; after, new shells of the task get a new token.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use natamux_proto::tasks::task::TaskState;

use crate::http::server::AppState;
use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::scheduler::{Scheduler, TimerId};

pub const REVOKE_GRACE_MS: u64 = 60_000;

type Pending = Arc<Mutex<HashMap<String, TimerId>>>;

/// Revokes the agent tokens of the daemon's tasks as they finish.
pub fn watch_tasks(state: &Arc<AppState>) {
    let terminals = state.terminals.clone();
    watch(&state.bus, Arc::clone(&state.scheduler), move |id| {
        terminals.revoke_agent(id)
    });
}

/// Calls `revoke` with the id of each task that stayed finished for the grace. Gives the
/// subscription id.
pub fn watch(
    bus: &EventBus,
    scheduler: Arc<Scheduler>,
    revoke: impl Fn(&str) + Send + Sync + 'static,
) -> u64 {
    let pending: Pending = Arc::default();
    let revoke = Arc::new(revoke);
    bus.subscribe(move |message| {
        let (id, finished) = match message {
            DaemonMessage::TaskCreated { task }
            | DaemonMessage::TaskUpdated { task, .. }
            | DaemonMessage::TaskChanged { task } => (
                task.id.as_str(),
                matches!(task.state, TaskState::Done | TaskState::Cancelled),
            ),
            // The delete revokes at once; a later task with the same id keeps its token.
            DaemonMessage::TaskDeleted { task_id } => (task_id.as_str(), false),
            _ => return,
        };
        let mut timers = pending.lock().unwrap_or_else(PoisonError::into_inner);
        if !finished {
            if let Some(timer) = timers.remove(id) {
                scheduler.cancel(timer);
            }
            return;
        }
        if timers.contains_key(id) {
            return;
        }
        let (task, revoke, slots) = (id.to_string(), Arc::clone(&revoke), Arc::clone(&pending));
        let timer = scheduler.after(REVOKE_GRACE_MS, move || {
            slots
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&task);
            revoke(&task);
        });
        timers.insert(id.to_string(), timer);
    })
}

#[cfg(test)]
mod tests {
    use natamux_proto::tasks::task::Task;

    use super::*;
    use crate::agents::task_access::AgentTokens;

    fn task(state: TaskState) -> DaemonMessage {
        DaemonMessage::TaskChanged {
            task: Task {
                id: "T-1".into(),
                state,
                ..Task::default()
            },
        }
    }

    fn watched() -> (Arc<EventBus>, Arc<Scheduler>, Arc<AgentTokens>) {
        let (bus, scheduler) = (EventBus::new(), Scheduler::manual());
        let tokens: Arc<AgentTokens> = Arc::default();
        let held = Arc::clone(&tokens);
        watch(&bus, Arc::clone(&scheduler), move |id| held.revoke(id));
        (bus, scheduler, tokens)
    }

    #[test]
    fn a_done_task_s_token_works_through_the_grace_then_answers_no_one() {
        let (bus, scheduler, tokens) = watched();
        let token = tokens.token_for("T-1");
        bus.publish(task(TaskState::Done));
        // More messages of the finished task do not move the deadline.
        scheduler.advance(30_000);
        bus.publish(task(TaskState::Done));
        scheduler.advance(REVOKE_GRACE_MS - 30_001);
        assert_eq!(tokens.task_of(&token).as_deref(), Some("T-1"));
        scheduler.advance(1);
        assert_eq!(tokens.task_of(&token), None);
        let reopened = tokens.token_for("T-1");
        assert_ne!(reopened, token);
        assert_eq!(tokens.task_of(&reopened).as_deref(), Some("T-1"));
    }

    #[test]
    fn a_cancelled_task_loses_it_too_and_a_reopen_in_the_grace_keeps_it() {
        let (bus, scheduler, tokens) = watched();
        let token = tokens.token_for("T-1");
        bus.publish(task(TaskState::Cancelled));
        scheduler.advance(REVOKE_GRACE_MS);
        assert_eq!(tokens.task_of(&token), None);

        let token = tokens.token_for("T-1");
        bus.publish(task(TaskState::Done));
        scheduler.advance(10_000);
        bus.publish(task(TaskState::Running));
        scheduler.advance(REVOKE_GRACE_MS);
        assert_eq!(tokens.task_of(&token).as_deref(), Some("T-1"));
    }
}
