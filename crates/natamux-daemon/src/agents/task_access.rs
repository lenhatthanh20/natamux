//! `AgentTokens`: one token per task for its shells (S-01), so an agent reaches its own task only.
//! The secrets live in memory for this daemon run; shells are the daemon's own, so a restart gives
//! new shells new tokens.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use subtle::ConstantTimeEq;

use crate::terminals::session_lifecycle::random_id;

#[derive(Default)]
pub struct AgentTokens {
    secrets: Mutex<HashMap<String, String>>,
}

impl AgentTokens {
    fn secrets(&self) -> MutexGuard<'_, HashMap<String, String>> {
        self.secrets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `<task id>.<secret>`, the same for every shell of the task in this run.
    pub fn token_for(&self, task_id: &str) -> String {
        let mut secrets = self.secrets();
        let secret = secrets
            .entry(task_id.to_string())
            .or_insert_with(|| random_id(24));
        format!("{task_id}.{secret}")
    }

    /// The task of a token this daemon made. Task ids have no dot, so the first one splits.
    pub fn task_of(&self, token: &str) -> Option<String> {
        let (task_id, secret) = token.split_once('.')?;
        let secrets = self.secrets();
        let known = secrets.get(task_id)?;
        // Constant time, so the time of a wrong guess tells nothing of the secret.
        bool::from(known.as_bytes().ct_eq(secret.as_bytes())).then(|| task_id.to_string())
    }

    /// A deleted task's token stops working, also for a later task with the same id.
    pub fn forget(&self, task_id: &str) {
        self.secrets().remove(task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_names_its_task_and_stays_the_same_for_the_run() {
        let tokens = AgentTokens::default();
        let a = tokens.token_for("T-1");
        assert!(a.starts_with("T-1.") && a.len() > 30);
        assert_eq!(tokens.token_for("T-1"), a);
        assert_ne!(tokens.token_for("T-2"), a);
        assert_eq!(tokens.task_of(&a).as_deref(), Some("T-1"));
    }

    #[test]
    fn refuses_a_wrong_secret_another_task_s_secret_and_an_unknown_task() {
        let tokens = AgentTokens::default();
        let a = tokens.token_for("T-1");
        let b = tokens.token_for("T-2");
        let secret_of_b = b.split_once('.').unwrap().1;
        assert_eq!(tokens.task_of(&format!("T-1.{secret_of_b}")), None);
        assert_eq!(tokens.task_of(&format!("{a}x")), None);
        assert_eq!(tokens.task_of("T-1."), None);
        assert_eq!(tokens.task_of("T-1"), None);
        assert_eq!(tokens.task_of("T-9.abc"), None);
        assert_eq!(tokens.task_of(""), None);
    }

    #[test]
    fn a_forgotten_task_s_token_stops_working_and_a_new_one_differs() {
        let tokens = AgentTokens::default();
        let old = tokens.token_for("T-1");
        tokens.forget("T-1");
        assert_eq!(tokens.task_of(&old), None);
        let new = tokens.token_for("T-1");
        assert_ne!(new, old);
        assert_eq!(tokens.task_of(&new).as_deref(), Some("T-1"));
    }
}
