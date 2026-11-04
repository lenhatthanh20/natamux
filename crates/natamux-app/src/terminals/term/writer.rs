//! What the parser writes a socket's bytes with (docs/plan-mit-license.md 6.2.2). It parses into
//! the screen it was made for, at most 16 KiB per lock, so a paint waits for one piece at most;
//! then it tells the event loop once. A writer made before the screen was replaced writes to the
//! old screen, which nothing shows any more, and its wake is dropped.

use std::sync::Arc;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::vte::ansi::Processor;

use super::event_loop::Listener;

/// The most output parsed under one lock of the screen.
pub const LOCK_PIECE: usize = 16 * 1024;

pub struct TermWriter {
    term: Arc<FairMutex<Term<Listener>>>,
    processor: Processor,
    listener: Listener,
}

impl TermWriter {
    pub(super) fn new(term: Arc<FairMutex<Term<Listener>>>, listener: Listener) -> Self {
        TermWriter {
            term,
            processor: Processor::new(),
            listener,
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        for piece in bytes.chunks(LOCK_PIECE) {
            let mut term = self.term.lock();
            self.processor.advance(&mut *term, piece);
        }
        self.listener.send_event(Event::Wakeup);
    }
}
