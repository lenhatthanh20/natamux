//! The workspace view's reads (git facts, ended terminals, events, branches) and its writes
//! (Push, Create PR, Create workspace, Start agent, Reopen agent, Stop), each for the task on
//! screen only.

use natamux_client::ClientError;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph};
use ui::prelude::*;

use super::WorkspaceView;
use crate::app::dashboard::{Dashboard, Written};
use crate::app::read_pace::{Pace, pace_ms, wait_of};
use crate::app::shortcuts::NewTerminal;
use crate::app::source::reason;
use crate::components::confirm::Confirm;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::push_toast;
use crate::git::commit_rules::{BUSY_REASON, UNDO_FAILED};
use crate::workspaces::workspace_rules::GitStep;
use crate::workspaces::wrap_up_rules::is_finished_state;

impl WorkspaceView {
    /// The header's and the banner's git facts: ahead, base, push, and the files not committed.
    pub(super) fn read_git(&mut self, cx: &mut Context<Self>) {
        self.read_git_at(false, cx);
    }

    /// Refresh (Shift+R) of Changes and History: past the daemon's 5 s cache, and the shown
    /// file again.
    pub fn read_git_fresh(&mut self, cx: &mut Context<Self>) {
        self.read_git_at(true, cx);
    }

    fn read_git_at(&mut self, fresh: bool, cx: &mut Context<Self>) {
        let Some(task) = self.task(cx).filter(|task| task.workspace.is_some()) else {
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let Some(shown) = self.shown.as_mut().filter(|s| s.task_id == task.id) else {
            return;
        };
        match shown.git_reads.ask(fresh, pace_ms()) {
            Pace::Now => {}
            Pace::Wait => return,
            Pace::After(wait) => {
                let id = task.id.clone();
                cx.spawn(async move |view, cx| {
                    cx.background_executor().timer(wait_of(wait)).await;
                    view.update(cx, |view, cx| {
                        let shown = view.shown.as_mut().filter(|s| s.task_id == id);
                        if let Some(fresh) = shown.and_then(|s| s.git_reads.timer_fired()) {
                            view.read_git_at(fresh, cx);
                        }
                    })
                    .ok();
                })
                .detach();
                return;
            }
        }
        let graph = source.read_graph(&task.id, fresh);
        let changes = source.read_changes(&task.id, fresh);
        let id = task.id.clone();
        self.changes.update(cx, |changes, cx| changes.read_file(cx));
        cx.spawn(async move |view, cx| {
            let (graph, changes) = (graph.await, changes.await);
            view.update(cx, |view, cx| view.git_read(&id, graph, changes, cx))
                .ok();
        })
        .detach();
    }

    /// Shows a git read when it differs from what is shown, then runs the read asked for
    /// meanwhile.
    fn git_read(
        &mut self,
        id: &str,
        graph: Result<GitGraph, ClientError>,
        changes: Result<Vec<ChangeEntry>, ClientError>,
        cx: &mut Context<Self>,
    ) {
        let Some(shown) = self.shown.as_mut().filter(|s| s.task_id == id) else {
            return;
        };
        let mut changed = false;
        // A failed read keeps what is shown.
        match graph {
            Ok(graph) if shown.graph.as_ref() != Some(&graph) || shown.graph_error.is_some() => {
                shown.graph = Some(graph);
                shown.graph_error = None;
                changed = true;
            }
            Err(error) if shown.graph.is_none() => {
                shown.graph_error = Some(reason(&error));
                changed = true;
            }
            _ => {}
        }
        match changes {
            Ok(changes) if shown.changes.as_ref() != Some(&changes) => {
                shown.changes = Some(changes);
                changed = true;
            }
            Err(error) if shown.changes.is_none() => {
                self.changes
                    .update(cx, |view, cx| view.set_error(reason(&error), cx));
            }
            _ => {}
        }
        let again = shown.git_reads.ended();
        if changed {
            self.sync_changes(cx);
            cx.notify();
        }
        if let Some(fresh) = again {
            self.read_git_at(fresh, cx);
        }
    }

    /// Hands the Changes tab what it shows of the task: its files and the git step that runs.
    pub(super) fn sync_changes(&mut self, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let (id, list, pending) = (shown.task_id.clone(), shown.changes.clone(), shown.pending);
        self.changes
            .update(cx, |changes, cx| changes.set_list(&id, list, pending, cx));
    }

    pub(super) fn read_ended(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.task(cx).filter(|task| is_finished_state(task.state)) else {
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.ended_shells(&task.id);
        cx.spawn(async move |view, cx| {
            let ended = answer.await.unwrap_or_default();
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut().filter(|s| s.task_id == task.id) {
                    shown.ended = ended;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn read_events(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.shown.as_ref().map(|s| s.task_id.clone()) else {
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.list_events(&id);
        cx.spawn(async move |view, cx| {
            let events = answer.await.ok();
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut().filter(|s| s.task_id == id) {
                    shown.events = events;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn read_branches(&mut self, task: &Task, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.list_branches(task.repo.as_deref());
        let id = task.id.clone();
        cx.spawn(async move |view, cx| {
            let branches = answer.await.ok();
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut().filter(|s| s.task_id == id) {
                    shown.branches = branches;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Push or Create PR of the header, the banner and the wrap-up.
    pub(super) fn git_step(&mut self, step: GitStep, cx: &mut Context<Self>) {
        let pull_request = step == GitStep::PullRequest;
        if let Some(answer) = self.run_git(
            step,
            move |dashboard, id, cx| {
                if pull_request {
                    dashboard.open_pull_request(id, cx)
                } else {
                    dashboard.push(id, cx)
                }
            },
            cx,
        ) {
            answer.detach();
        }
    }

    /// Runs one git step of the task on screen, None while another runs: Push, Create PR,
    /// Commit all, Roll back, Undo. The git facts are read again after it.
    pub fn run_git<T: 'static>(
        &mut self,
        step: GitStep,
        start: impl FnOnce(&mut Dashboard, &str, &mut Context<Dashboard>) -> Written<T>,
        cx: &mut Context<Self>,
    ) -> Option<Written<T>> {
        let shown = self.shown.as_mut().filter(|s| s.pending.is_none())?;
        shown.pending = Some(step);
        let id = shown.task_id.clone();
        let answer = self
            .dashboard
            .update(cx, |dashboard, cx| start(dashboard, &id, cx));
        self.sync_changes(cx);
        cx.notify();
        Some(cx.spawn(async move |view, cx| {
            let result = answer.await;
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut() {
                    shown.pending = None;
                }
                view.read_git(cx);
                cx.notify();
            })
            .ok();
            result
        }))
    }

    /// Undo of a rollback's toast. In the task's workspace it is one git step, so Commit all,
    /// Push and Roll back wait for it; another task's runs at once.
    pub(super) fn undo_rollback(&mut self, task_id: &str, cx: &mut Context<Self>) {
        let ours = self.shown.as_ref().is_some_and(|s| s.task_id == task_id);
        if !ours {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.undo_rollback(task_id, cx))
                .detach();
            return;
        }
        let started = self.run_git(
            GitStep::UndoRollback,
            |dashboard, id, cx| dashboard.undo_rollback(id, cx),
            cx,
        );
        match started {
            Some(answer) => answer.detach(),
            None => push_toast(
                UNDO_FAILED,
                ToastTone::Error,
                Some(BUSY_REASON.to_owned()),
                Vec::new(),
                cx,
            ),
        }
    }

    /// Create workspace, or Start agent with `agent`.
    pub(super) fn start(&mut self, agent: Option<String>, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut().filter(|s| !s.starting) else {
            return;
        };
        shown.starting = true;
        let (id, base) = (shown.task_id.clone(), shown.base.clone());
        let answer = self.dashboard.update(cx, |dashboard, cx| match &agent {
            Some(agent) => dashboard.start_agent_from(&id, agent, base, cx),
            None => dashboard.create_workspace(&id, base, cx),
        });
        cx.spawn(async move |view, cx| {
            answer.await.ok();
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut() {
                    shown.starting = false;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn reopen(&mut self, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut().filter(|s| !s.reopening) else {
            return;
        };
        shown.reopening = true;
        let id = shown.task_id.clone();
        let answer = self
            .dashboard
            .update(cx, |dashboard, cx| dashboard.reopen_agent(&id, cx));
        cx.spawn(async move |view, cx| {
            let reopened = answer.await.is_ok();
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut() {
                    shown.reopening = false;
                    shown.shells_wanted |= reopened;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Stop of the wrap-up: asks first, as the program in it ends too.
    pub(super) fn stop(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task_id) = self.shown.as_ref().map(|s| s.task_id.clone()) else {
            return;
        };
        let program = self
            .dashboard
            .read(cx)
            .data
            .terminals
            .get(&task_id)
            .and_then(|list| list.programs.get(name).cloned().flatten())
            .unwrap_or_else(|| "terminal".into());
        let dashboard = self.dashboard.clone();
        let name = name.to_owned();
        Confirm::new(
            format!("Stop {program} in Terminal {name}?"),
            "The terminal ends with it.",
            "Stop",
            move |_, cx| {
                let failure = format!("Could not stop Terminal {name}.");
                dashboard.update(cx, |dashboard, cx| {
                    dashboard
                        .end_terminal(&task_id, &name, failure, cx)
                        .detach()
                });
            },
        )
        .open(window, cx);
    }

    pub(super) fn show_terminals(&mut self, cx: &mut Context<Self>) {
        if let Some(shown) = self.shown.as_mut() {
            shown.shells_wanted = true;
        }
        cx.notify();
    }

    pub(super) fn new_terminal(&mut self, _: &NewTerminal, _: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.task(cx).filter(|task| task.workspace.is_some()) else {
            return;
        };
        if is_finished_state(task.state) {
            self.show_terminals(cx);
        }
        self.panes.update(cx, |panes, cx| panes.add(cx));
    }
}
