//! The writes of `view.json`: the newest view state, off the main thread, one write at a time.
//! How long a change waits depends on who made it (`Wait`); the app's quit writes what still
//! waits.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ui::prelude::*;

use super::Dashboard;
use crate::app::view_state::{self, ViewState};

/// Changes that come together are one write.
const SOON: Duration = Duration::from_millis(300);
/// Each turn start of each agent makes a seen mark: they share a write.
const LATER: Duration = Duration::from_secs(5);

/// How long a change of the view state may wait for its write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wait {
    /// A change the user made.
    Soon,
    /// A seen mark the feed made. Only an unread dot is lost when the app crashes first.
    Later,
    /// A panel width: written when its drag ended, not on each move of the pointer.
    Settled,
}

/// The changes that wait for a write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Pending {
    /// The earliest time a change of `Soon` or `Later` wants its write.
    due: Option<Instant>,
    /// When the last width change is `SOON` old.
    settles: Option<Instant>,
}

/// What the save loop does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Nothing waits: the loop ends.
    Idle,
    Sleep(Duration),
    Write,
}

impl Pending {
    fn add(&mut self, wait: Wait, now: Instant) {
        let earlier = |due: Option<Instant>, at: Instant| Some(due.map_or(at, |due| due.min(at)));
        match wait {
            Wait::Soon => self.due = earlier(self.due, now + SOON),
            Wait::Later => self.due = earlier(self.due, now + LATER),
            // Each move of the edge waits anew.
            Wait::Settled => self.settles = Some(now + SOON),
        }
    }

    /// When the first write is wanted.
    fn next(&self) -> Option<Instant> {
        match (self.due, self.settles) {
            (Some(due), Some(settles)) => Some(due.min(settles)),
            (due, settles) => due.or(settles),
        }
    }

    /// `dragging`: a drag runs, so a width that stopped changing may change again.
    fn step(&mut self, now: Instant, dragging: bool) -> Step {
        if dragging && self.settles.is_some_and(|at| at <= now) {
            self.settles = Some(now + SOON);
        }
        match self.next() {
            None => Step::Idle,
            Some(at) if at > now => Step::Sleep(at.duration_since(now)),
            Some(_) => {
                *self = Pending::default();
                Step::Write
            }
        }
    }
}

/// The save loop of one dashboard.
#[derive(Default)]
pub(super) struct Saves {
    pending: Pending,
    /// The file is being written: a new change waits for the loop's next round.
    writing: bool,
    /// When the sleeping loop wakes. A change that is due earlier starts a new loop.
    wakes: Option<Instant>,
    /// The number of the last write asked for, and the newest one that reached the file.
    turn: u64,
    newest: Arc<Mutex<u64>>,
    task: Option<gpui::Task<()>>,
}

/// What the loop got from one look at the dashboard.
enum Next {
    Idle,
    Sleep(Duration),
    Write(Box<Job>),
}

/// One write, as a background thread makes it.
struct Job {
    turn: u64,
    newest: Arc<Mutex<u64>>,
    folder: PathBuf,
    view: ViewState,
}

/// One write at a time, and never an older state over a newer one: the write at quit does not
/// wait for one that is still on its way to a background thread.
fn write(newest: &Mutex<u64>, turn: u64, folder: &Path, view: &ViewState) -> std::io::Result<()> {
    let mut newest = newest.lock().unwrap_or_else(PoisonError::into_inner);
    if turn < *newest {
        return Ok(());
    }
    *newest = turn;
    view_state::save(folder, view)
}

fn report(saved: std::io::Result<()>) {
    if let Err(error) = saved {
        eprintln!("natamux-app: could not save the view state: {error}");
    }
}

impl Dashboard {
    /// Writes the newest view state after a short wait.
    pub(super) fn save_view(&mut self, cx: &mut Context<Self>) {
        self.save_view_after(Wait::Soon, cx);
    }

    pub(super) fn save_view_after(&mut self, wait: Wait, cx: &mut Context<Self>) {
        if self.view_folder.is_none() {
            return;
        }
        self.saves.pending.add(wait, cx.background_executor().now());
        let early = match (self.saves.pending.next(), self.saves.wakes) {
            (Some(next), Some(wakes)) => next < wakes,
            _ => false,
        };
        // A loop that sleeps past the new change is replaced; one that writes looks again
        // after its write.
        if self.saves.task.is_none() || (early && !self.saves.writing) {
            self.start_saves(cx);
        }
    }

    fn start_saves(&mut self, cx: &mut Context<Self>) {
        self.saves.wakes = None;
        self.saves.task = Some(cx.spawn(async move |this, cx| {
            loop {
                match this.update(cx, |this, cx| this.save_step(cx)) {
                    Ok(Next::Sleep(wait)) => cx.background_executor().timer(wait).await,
                    Ok(Next::Write(job)) => {
                        let saved = cx
                            .background_executor()
                            .spawn(async move {
                                write(&job.newest, job.turn, &job.folder, &job.view)
                            })
                            .await;
                        report(saved);
                        let done = this.update(cx, |this, _| this.saves.writing = false);
                        if done.is_err() {
                            break;
                        }
                    }
                    Ok(Next::Idle) | Err(_) => break,
                }
            }
        }));
    }

    fn save_step(&mut self, cx: &mut Context<Self>) -> Next {
        let now = cx.background_executor().now();
        let step = self.saves.pending.step(now, cx.has_active_drag());
        let folder = self.view_folder.clone();
        match (step, folder) {
            (Step::Sleep(wait), _) => {
                self.saves.wakes = Some(now + wait);
                Next::Sleep(wait)
            }
            (Step::Write, Some(folder)) => {
                self.saves.wakes = None;
                self.saves.writing = true;
                self.saves.turn += 1;
                Next::Write(Box::new(Job {
                    turn: self.saves.turn,
                    newest: self.saves.newest.clone(),
                    folder,
                    view: self.view.clone(),
                }))
            }
            (Step::Idle, _) | (Step::Write, None) => {
                self.saves.wakes = None;
                self.saves.task = None;
                Next::Idle
            }
        }
    }

    /// The app quits: what still waits is written now, on this thread, so no wait loses a
    /// change.
    pub(super) fn flush_view(&mut self) {
        let waits = self.saves.pending.next().is_some() || self.saves.writing;
        // Dropped first: a write the loop has not started is not made after this one.
        self.saves.task = None;
        self.saves.pending = Pending::default();
        self.saves.wakes = None;
        self.saves.writing = false;
        let Some(folder) = self.view_folder.as_ref().filter(|_| waits) else {
            return;
        };
        self.saves.turn += 1;
        report(write(&self.saves.newest, self.saves.turn, folder, &self.view));
    }
}

#[cfg(test)]
#[path = "dashboard_saves_tests.rs"]
mod tests;
