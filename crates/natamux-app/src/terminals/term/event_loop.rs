//! The event loop of an own terminal (docs/plan-mit-license.md 6.2.4). A screen's events come
//! through a channel, tagged with the screen they belong to. The first event after a quiet time
//! is handled at once; then the events are taken in batches, one per pace window, until a whole
//! window brings none. So a steady stream of output wakes the view once per pace. The model
//! drops the events of an older screen when it applies a batch.

use std::future::Future;
use std::pin::pin;

use alacritty_terminal::event::{Event, EventListener};
use futures::channel::mpsc::UnboundedSender;
use futures::future::{self, Either};
use futures::{Stream, StreamExt};

use crate::terminals::terminal_parser::YieldNow;

/// More waiting events than this, not counting wakes, end a pace window early.
const MAX_WAITING: usize = 100;

/// Sends a screen's events to the loop, tagged with the screen's generation.
#[derive(Clone)]
pub struct Listener {
    events: UnboundedSender<(u64, Event)>,
    generation: u64,
}

impl Listener {
    pub fn new(events: UnboundedSender<(u64, Event)>, generation: u64) -> Self {
        Listener { events, generation }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        // The loop ends with its model; nothing is told then.
        self.events.unbounded_send((self.generation, event)).ok();
    }
}

/// An event and the generation of the screen that sent it.
pub type Tagged = (u64, Event);

/// Runs until the events end or `apply` says the model is gone. `pace` makes the timer of one
/// pace window. `apply` gets the wake of the newest screen first, then the other events in the
/// order they came.
pub async fn run<T: Future<Output = ()>>(
    events: impl Stream<Item = Tagged>,
    mut pace: impl FnMut() -> T,
    mut apply: impl FnMut(Vec<Tagged>) -> bool,
) {
    let mut events = pin!(events);
    while let Some(first) = events.next().await {
        if !apply(vec![first]) {
            return;
        }
        loop {
            let (batch, closed) = gather(&mut events, pace()).await;
            if batch.is_empty() {
                if closed {
                    return;
                }
                // A quiet window: the next event is handled at once again.
                YieldNow(false).await;
                break;
            }
            if !apply(batch) || closed {
                return;
            }
            YieldNow(false).await;
        }
    }
}

/// The events of one pace window, and whether the channel closed.
async fn gather<S: Stream<Item = Tagged> + Unpin>(
    events: &mut S,
    timer: impl Future<Output = ()>,
) -> (Vec<Tagged>, bool) {
    let mut timer = pin!(timer);
    // One wake is enough; the one of the newest screen is kept, as an older one is dropped.
    let mut wake: Option<u64> = None;
    let mut others = Vec::new();
    loop {
        let closed = match future::select(timer.as_mut(), events.next()).await {
            Either::Left(_) => false,
            Either::Right((None, _)) => true,
            Either::Right((Some((generation, Event::Wakeup)), _)) => {
                wake = wake.max(Some(generation));
                continue;
            }
            Either::Right((Some(event), _)) => {
                others.push(event);
                if others.len() <= MAX_WAITING {
                    continue;
                }
                false
            }
        };
        let wake = wake.map(|generation| (generation, Event::Wakeup));
        return (wake.into_iter().chain(others).collect(), closed);
    }
}
