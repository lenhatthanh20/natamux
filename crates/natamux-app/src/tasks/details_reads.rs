//! The panel's reads for the task it shows (the web's `['stage-events', id]`, `['settings',
//! repo]`, `['stage-files', id]` and the branch list): on show, after a logged change and on the
//! 60 s poll.

use ui::prelude::*;

use super::{DetailsPanel, Shown};
use crate::app::settings_reads::SettingsReads;
use crate::app::source::reason;
use crate::workspaces::wrap_up_rules::is_finished_state;

impl DetailsPanel {
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

    /// Runs `answer` for the task on screen and keeps the result with `keep`.
    fn read_into<T: 'static>(
        &mut self,
        answer: natamux_client::Answer<T>,
        keep: impl FnOnce(&mut Shown, Result<T, String>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.shown.as_ref().map(|shown| shown.task_id.clone()) else {
            return;
        };
        cx.spawn(async move |panel, cx| {
            let result = answer.await.map_err(|error| reason(&error));
            panel
                .update(cx, |panel, cx| {
                    if let Some(shown) = panel.shown.as_mut().filter(|s| s.task_id == id) {
                        keep(shown, result);
                        cx.notify();
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
        let answer = source.list_events(&task.id);
        // A read that fails keeps the events read before (`keepPreviousData`).
        self.read_into(
            answer,
            |shown, events| {
                if events.is_ok() || shown.events.is_none() {
                    shown.events = Some(events);
                }
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
        let answer = source.list_stage_files(&task.id);
        self.read_into(answer, |shown, files| shown.files = Some(files), cx);
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
        let answer = source.load_settings(&repo);
        self.read_into(
            answer,
            |shown, settings| {
                if settings.is_ok() || shown.settings.is_none() {
                    shown.settings = Some(settings);
                }
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
        self.read_into(answer, |shown, branches| shown.branches = branches.ok(), cx);
    }
}
