//! The workspace view's reads (git facts, ended terminals, events, branches) and its writes
//! (Push, Create PR, Create workspace, Start agent, Reopen agent, Stop), each for the task on
//! screen only.

use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::WorkspaceView;
use crate::app::shortcuts::NewTerminal;
use crate::components::confirm::Confirm;
use crate::workspaces::workspace_rules::GitStep;
use crate::workspaces::wrap_up_rules::is_finished_state;

impl WorkspaceView {
    /// The header's and the banner's git facts: ahead, base, push, and the files not committed.
    pub(super) fn read_git(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.task(cx).filter(|task| task.workspace.is_some()) else {
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let (graph, changes) = (source.graph(&task.id), source.list_changes(&task.id));
        let id = task.id.clone();
        cx.spawn(async move |view, cx| {
            let (graph, changes) = (graph.await.ok(), changes.await.ok());
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut().filter(|s| s.task_id == id) {
                    shown.graph = graph;
                    shown.changes = changes;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
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

    /// Push or Create PR; one git step at a time.
    pub(super) fn git_step(&mut self, step: GitStep, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut().filter(|s| s.pending.is_none()) else {
            return;
        };
        shown.pending = Some(step);
        let id = shown.task_id.clone();
        let answer = self.dashboard.update(cx, |dashboard, cx| match step {
            GitStep::PullRequest => dashboard.open_pull_request(&id, cx),
            _ => dashboard.push(&id, cx),
        });
        cx.spawn(async move |view, cx| {
            answer.await.ok();
            view.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut() {
                    shown.pending = None;
                }
                view.read_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
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
