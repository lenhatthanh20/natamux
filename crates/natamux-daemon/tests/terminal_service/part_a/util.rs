//! Small helpers shared by the groups of part A.

use std::sync::Arc;

use natamux_daemon::terminals::service::AttachOptions;
use natamux_daemon::terminals::shells::Detach;
use natamux_daemon::terminals::types::ClientRef;

use crate::support::{Harness, Recorder, SIZE, Size, client};

/// A new client attached to `terminal` of `task`.
pub fn attach_at(
    h: &Harness,
    task: &str,
    size: Size,
    terminal: &str,
) -> (Arc<Recorder>, ClientRef, Detach) {
    let (recorder, client_ref) = client();
    let detach = h.attach_to(task, &client_ref, size, terminal, AttachOptions::default());
    (recorder, client_ref, detach)
}

/// A new client of the default size on terminal "1" of T-0001.
pub fn attach_one(h: &Harness) -> (Arc<Recorder>, ClientRef, Detach) {
    attach_at(h, "T-0001", SIZE, "1")
}

pub fn signals(h: &Harness, index: usize) -> Vec<natamux_pty::Signal> {
    h.pty(index).signals()
}
