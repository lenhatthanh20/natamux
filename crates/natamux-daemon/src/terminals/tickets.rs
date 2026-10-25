//! One-time tickets for the terminal socket (`issueTicket`, `redeemTicket`). A browser cannot
//! set headers on a WebSocket: instead of the daemon token in the URL, it asks for a ticket with
//! the token, then opens the socket with the ticket. A ticket works once.

use std::collections::HashMap;

use subtle::ConstantTimeEq;

use super::limits::TICKET_TTL_MS;
use super::session_lifecycle::random_id;

#[derive(Default)]
pub struct Tickets {
    open: HashMap<String, (String, u64)>,
}

impl Tickets {
    pub fn issue(&mut self, task_id: &str, now: u64) -> String {
        self.drop_expired(now);
        let ticket = random_id(32);
        self.open
            .insert(ticket.clone(), (task_id.to_string(), now + TICKET_TTL_MS));
        ticket
    }

    /// Compared in constant time, so the time of a wrong guess tells nothing.
    pub fn redeem(&mut self, ticket: &str, task_id: &str, now: u64) -> bool {
        self.drop_expired(now);
        let found = self
            .open
            .keys()
            .find(|stored| bool::from(stored.as_bytes().ct_eq(ticket.as_bytes())))
            .cloned();
        match found.and_then(|stored| self.open.remove(&stored)) {
            Some((task, _)) => task == task_id,
            None => false,
        }
    }

    fn drop_expired(&mut self, now: u64) {
        self.open.retain(|_, (_, expires_at)| *expires_at > now);
    }
}
