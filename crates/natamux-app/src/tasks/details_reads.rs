//! The panel's reads for the task it shows (the web's `['stage-events', id]`, `['settings',
//! repo]`, `['stage-files', id]` and the branch list): on show, after a logged change and on the
//! 60 s poll. An agent logs many lines a second: each kind reads at most once a second.

use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::{DetailsPanel, Shown};
use crate::app::clock::{follow_labels, now_ms};
use crate::app::read_pace::{Pace, ReadPace, pace_ms, wait_of};
use crate::app::settings_reads::SettingsReads;
use crate::app::source::reason;
use crate::tasks::details_rules::details_wait;
use crate::workspaces::wrap_up_rules::is_finished_state;

/// The kinds of read that follow the task's log.
#[derive(Clone, Copy)]
enum Kind {
    Events,
    Files,
    Settings,
}

#[derive(Debug, Default)]
pub struct ReadPaces {
    events: ReadPace,
    files: ReadPace,
    settings: ReadPace,
}

impl Kind {
    fn of(self, shown: &mut Shown) -> &mut ReadPace {
        match self {
            Kind::Events => &mut shown.paces.events,
            Kind::Files => &mut shown.paces.files,
            Kind::Settings => &mut shown.paces.settings,
        }
    }

    fn read(self, panel: &mut DetailsPanel, cx: &mut Context<DetailsPanel>) {
        match self {
            Kind::Events => panel.read_events(cx),
            Kind::Files => panel.read_files(cx),
            Kind::Settings => panel.read_settings(cx),
        }
    }
}

/// Keeps `new` and tells if it differs from what was kept.
fn keep_new<T: PartialEq>(kept: &mut T, new: T) -> bool {
    let changed = *kept != new;
    *kept = new;
    changed
}

impl DetailsPanel {
    /// Draws the panel again when a time it shows changes by the clock alone (`follow_labels`).
    pub(super) fn follow_times(&mut self, task: &Task, cx: &mut Context<Self>) {
        let now = now_ms();
        let progress = self.progress(task).map(|(_, progress)| progress);
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        let events = shown.events.as_ref().and_then(|read| read.as_ref().ok());
        let times = events
            .into_iter()
            .flatten()
            .map(|stored| stored.event.at.as_str());
        let wait = details_wait(task, times, progress.as_deref().unwrap_or_default(), now);
        shown.clock.set_for(wait.map(|wait| now + wait));
        let wait = shown.clock.at.map(|at| at - now);
        follow_labels(&mut shown.clock.timer, wait, cx, |panel, cx| {
            if let Some(shown) = panel.shown.as_mut() {
                shown.clock.timer = None;
            }
            cx.notify();
        });
    }

    pub(super) fn read_polled(&mut self, cx: &mut Context<Self>) {
        self.read_log(cx);
        self.refresh_tree(cx);
    }

    /// The window has focus again: the poll it skipped reads once now, if the panel still shows.
    pub fn read_missed(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.missed_poll) && self.is_open(cx) {
            self.read_polled(cx);
        }
    }

    /// Whether to read `kind` now. A read asked for too soon runs by itself at the end of the
    /// second.
    fn pace(&mut self, kind: Kind, cx: &mut Context<Self>) -> bool {
        let Some(shown) = self.shown.as_mut() else {
            return false;
        };
        let wait = match kind.of(shown).ask(false, pace_ms()) {
            Pace::Now => return true,
            Pace::Wait => return false,
            Pace::After(wait) => wait,
        };
        let id = shown.task_id.clone();
        cx.spawn(async move |panel, cx| {
            cx.background_executor().timer(wait_of(wait)).await;
            panel
                .update(cx, |panel, cx| {
                    let shown = panel.shown.as_mut().filter(|s| s.task_id == id);
                    if shown.and_then(|s| kind.of(s).timer_fired()).is_some() {
                        kind.read(panel, cx);
                    }
                })
                .ok();
        })
        .detach();
        false
    }

    pub(super) fn read_all(&mut self, cx: &mut Context<Self>) {
        self.read_log(cx);
        self.read_branches(cx);
    }

    /// Everything a new line in the task's log may change: its events, the stage files and the
    /// project's stages.
    pub(super) fn read_log(&mut self, cx: &mut Context<Self>) {
        self.read_events(cx);
        self.read_files(cx);
        self.read_settings(cx);
    }

    /// Runs `answer` for the task on screen and keeps the result with `keep`, which tells if
    /// it changed what shows. Then the read of `kind` asked for meanwhile runs.
    fn read_into<T: 'static>(
        &mut self,
        answer: natamux_client::Answer<T>,
        kind: Option<Kind>,
        keep: impl FnOnce(&mut Shown, Result<T, String>) -> bool + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.shown.as_ref().map(|shown| shown.task_id.clone()) else {
            return;
        };
        cx.spawn(async move |panel, cx| {
            let result = answer.await.map_err(|error| reason(&error));
            panel
                .update(cx, |panel, cx| {
                    let Some(shown) = panel.shown.as_mut().filter(|s| s.task_id == id) else {
                        return;
                    };
                    let changed = keep(shown, result);
                    let again = kind.and_then(|kind| kind.of(shown).ended().map(|_| kind));
                    if changed {
                        cx.notify();
                    }
                    if let Some(kind) = again {
                        kind.read(panel, cx);
                    }
                })
                .ok();
        })
        .detach();
    }

    fn read_events(&mut self, cx: &mut Context<Self>) {
        let (Some(task), Some(source)) = (self.task(cx), self.dashboard.read(cx).source().cloned())
        else {
            return;
        };
        if !self.pace(Kind::Events, cx) {
            return;
        }
        let answer = source.list_events(&task.id);
        // A read that fails keeps the events read before (`keepPreviousData`).
        self.read_into(
            answer,
            Some(Kind::Events),
            |shown, events| {
                (events.is_ok() || shown.events.is_none())
                    && keep_new(&mut shown.events, Some(events))
            },
            cx,
        );
    }

    /// No workspace: no files (the web's `[]`).
    pub(super) fn read_files(&mut self, cx: &mut Context<Self>) {
        let (Some(task), Some(source)) = (self.task(cx), self.dashboard.read(cx).source().cloned())
        else {
            return;
        };
        if task.workspace.is_none() {
            if let Some(shown) = self.shown.as_mut() {
                shown.files = Some(Ok(Vec::new()));
            }
            return;
        }
        if !self.pace(Kind::Files, cx) {
            return;
        }
        let answer = source.list_stage_files(&task.id);
        let keep = |shown: &mut Shown, files| keep_new(&mut shown.files, Some(files));
        self.read_into(answer, Some(Kind::Files), keep, cx);
    }

    fn read_settings(&mut self, cx: &mut Context<Self>) {
        let (Some(task), Some(source)) = (self.task(cx), self.dashboard.read(cx).source().cloned())
        else {
            return;
        };
        let repo = task.repo.clone().unwrap_or_else(|| "default".into());
        if let Some(reads) = SettingsReads::global(cx) {
            reads.update(cx, |reads, _| reads.watch(&repo));
        }
        if !self.pace(Kind::Settings, cx) {
            return;
        }
        let answer = source.load_settings(&repo);
        self.read_into(
            answer,
            Some(Kind::Settings),
            |shown, settings| {
                (settings.is_ok() || shown.settings.is_none())
                    && keep_new(&mut shown.settings, Some(settings))
            },
            cx,
        );
    }

    /// The settings of a project were read again (a save, the window active again): the shown
    /// task reads its own copy again when it is of that project.
    pub(super) fn settings_refreshed(&mut self, repo: &str, cx: &mut Context<Self>) {
        let of_repo = self
            .task(cx)
            .is_some_and(|task| task.repo.as_deref().unwrap_or("default") == repo);
        if of_repo {
            self.read_settings(cx);
        }
    }

    /// "Start from branch" of a task with no workspace.
    fn read_branches(&mut self, cx: &mut Context<Self>) {
        let (Some(task), Some(source)) = (self.task(cx), self.dashboard.read(cx).source().cloned())
        else {
            return;
        };
        if task.workspace.is_some() || is_finished_state(task.state) {
            return;
        }
        let answer = source.list_branches(task.repo.as_deref());
        let keep = |shown: &mut Shown, branches: Result<_, _>| {
            keep_new(&mut shown.branches, branches.ok())
        };
        self.read_into(answer, None, keep, cx);
    }
}
