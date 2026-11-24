//! One-time tickets for the terminal socket (`issueTicket`, `redeemTicket`). A browser cannot
//! set headers on a WebSocket: instead of the daemon token in the URL, it asks for a ticket with
//! the token, then opens the socket with the ticket. A ticket works once.

use std::collections::HashMap;

use subtle::ConstantTimeEq;

use super::limits::{MAX_OPEN_TICKETS, TICKET_TTL_MS};
use super::session_lifecycle::random_id;

struct Open {
    task_id: String,
    /// The task whose agent token asked for it; None: a person.
    issuer: Option<String>,
    expires_at: u64,
}

#[derive(Default)]
pub struct Tickets {
    open: HashMap<String, Open>,
}

impl Tickets {
    /// None when too many tickets are open: a flood of requests cannot grow the table.
    pub fn issue(&mut self, task_id: &str, issuer: Option<&str>, now: u64) -> Option<String> {
        self.drop_expired(now);
        if self.open.len() >= MAX_OPEN_TICKETS {
            return None;
        }
        let ticket = random_id(32);
        let open = Open {
            task_id: task_id.to_string(),
            issuer: issuer.map(str::to_string),
            expires_at: now + TICKET_TTL_MS,
        };
        self.open.insert(ticket.clone(), open);
        Some(ticket)
    }

    /// Compared in constant time, so the time of a wrong guess tells nothing. A ticket for
    /// another task is spent too: it works once, whatever it was tried on.
    pub fn redeem(&mut self, ticket: &str, task_id: &str, now: u64) -> bool {
        self.redeem_issuer(ticket, task_id, now).is_some()
    }

    /// `redeem`, giving who asked for the ticket: the task of an agent token, or None for a
    /// person. None outside: not redeemed.
    pub fn redeem_issuer(
        &mut self,
        ticket: &str,
        task_id: &str,
        now: u64,
    ) -> Option<Option<String>> {
        self.drop_expired(now);
        let found = self
            .open
            .keys()
            .find(|stored| bool::from(stored.as_bytes().ct_eq(ticket.as_bytes())))
            .cloned();
        match found.and_then(|stored| self.open.remove(&stored)) {
            Some(open) if open.task_id == task_id => Some(open.issuer),
            _ => None,
        }
    }

    /// The tickets the agent token of `task_id` asked for, once that token is revoked.
    pub fn drop_issued_by(&mut self, task_id: &str) {
        self.open
            .retain(|_, open| open.issuer.as_deref() != Some(task_id));
    }

    fn drop_expired(&mut self, now: u64) {
        self.open.retain(|_, open| open.expires_at > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_a_ticket_above_the_cap_until_some_expire() {
        let mut tickets = Tickets::default();
        for _ in 0..MAX_OPEN_TICKETS {
            assert!(tickets.issue("T-0001", None, 0).is_some());
        }
        assert!(tickets.issue("T-0001", None, 1).is_none());
        assert!(tickets.issue("T-0001", None, TICKET_TTL_MS).is_some());
    }

    #[test]
    fn a_ticket_works_once_and_a_wrong_task_spends_it() {
        let mut tickets = Tickets::default();
        let ticket = tickets.issue("T-0001", None, 0).unwrap();
        assert!(tickets.redeem(&ticket, "T-0001", 1));
        assert!(!tickets.redeem(&ticket, "T-0001", 2));
        let tried = tickets.issue("T-0001", Some("T-0001"), 0).unwrap();
        assert!(!tickets.redeem(&tried, "T-0002", 1));
        assert!(!tickets.redeem(&tried, "T-0001", 2));
    }

    #[test]
    fn an_expired_ticket_is_refused() {
        let mut tickets = Tickets::default();
        let ticket = tickets.issue("T-0001", None, 0).unwrap();
        assert!(!tickets.redeem(&ticket, "T-0001", TICKET_TTL_MS));
    }

    #[test]
    fn a_revoked_agent_s_tickets_go_and_a_person_s_stay() {
        let mut tickets = Tickets::default();
        let agent = tickets.issue("T-0001", Some("T-0001"), 0).unwrap();
        let person = tickets.issue("T-0001", None, 0).unwrap();
        tickets.drop_issued_by("T-0001");
        assert!(!tickets.redeem(&agent, "T-0001", 1));
        assert!(tickets.redeem(&person, "T-0001", 1));
    }
}
