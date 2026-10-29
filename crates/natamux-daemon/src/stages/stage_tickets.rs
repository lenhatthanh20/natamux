//! One-time result tickets of command stages (S-01). The script of a command stage reports
//! `natamux stage pass` with its task's agent token, which may not pass a stage; the ticket in its
//! script lets that one report through, once. A new run of the stage replaces the ticket.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use subtle::ConstantTimeEq;

use crate::terminals::session_lifecycle::random_id;

/// By task: the stage and the ticket of its last command run.
#[derive(Default)]
pub struct StageTickets {
    by_task: Mutex<HashMap<String, (String, String)>>,
}

impl StageTickets {
    fn by_task(&self) -> MutexGuard<'_, HashMap<String, (String, String)>> {
        self.by_task.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new ticket for the command run of the stage; the task's earlier one stops working.
    pub fn issue(&self, task_id: &str, stage_id: &str) -> String {
        let ticket = random_id(24);
        self.by_task()
            .insert(task_id.into(), (stage_id.into(), ticket.clone()));
        ticket
    }

    /// True once for the ticket of that task and stage: a match uses it up.
    pub fn redeem(&self, task_id: &str, stage_id: &str, ticket: &str) -> bool {
        let mut by_task = self.by_task();
        let matches = by_task.get(task_id).is_some_and(|(stage, known)| {
            stage == stage_id && bool::from(known.as_bytes().ct_eq(ticket.as_bytes()))
        });
        if matches {
            by_task.remove(task_id);
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_passes_once_for_its_task_and_stage_only() {
        let tickets = StageTickets::default();
        let ticket = tickets.issue("T-1", "testing");
        assert!(!tickets.redeem("T-2", "testing", &ticket));
        assert!(!tickets.redeem("T-1", "release", &ticket));
        assert!(!tickets.redeem("T-1", "testing", "wrong"));
        assert!(tickets.redeem("T-1", "testing", &ticket));
        assert!(!tickets.redeem("T-1", "testing", &ticket));
    }

    #[test]
    fn a_new_run_replaces_the_ticket() {
        let tickets = StageTickets::default();
        let old = tickets.issue("T-1", "testing");
        let new = tickets.issue("T-1", "testing");
        assert_ne!(old, new);
        assert!(!tickets.redeem("T-1", "testing", &old));
        assert!(tickets.redeem("T-1", "testing", &new));
    }
}
