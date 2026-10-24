//! The parts of the workspace view: the no-workspace view and the finished summary, the
//! workspace itself with the body of its tab, and what the finished banner's and the wrap-up's
//! buttons do.

use std::rc::Rc;

use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::{Shown, WorkspaceView};
use crate::agents::agent_choices::{DEFAULT_AGENT, is_agent_id};
use crate::app::view_rules::WorkspaceTab;
use crate::git::history_view::HistoryView;
use crate::tasks::state_filter::width_probe;
use crate::tasks::table_rules::waits_for;
use crate::workspaces::finished_banner::{FinishedActions, FinishedBanner, OnClick};
use crate::workspaces::finished_summary::{FinishedSummary, follow_up_title};
use crate::workspaces::no_workspace::NoWorkspace;
use crate::workspaces::workspace_header::{GitButtons, ICON_ONLY_BELOW};
use crate::workspaces::workspace_rules::GitStep;
use crate::workspaces::wrap_up::{WrapUp, WrapUpActions};
use crate::workspaces::wrap_up_rules::{GitFacts, is_finished_state};
use crate::workspaces::{new_workspace_dialog, remove_workspace_dialog};

impl WorkspaceView {
    pub(super) fn render_no_workspace(
        &self,
        task: Task,
        shown: &Shown,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if is_finished_state(task.state) {
            let dashboard = self.dashboard.clone();
            let (repo, title) = (task.repo.clone(), follow_up_title(&task.id));
            let (progress, id) = (self.dashboard.clone(), task.id.clone());
            return FinishedSummary {
                task,
                events: shown.events.clone(),
                on_progress: Rc::new(move |_: &mut Window, cx: &mut App| {
                    progress.update(cx, |dashboard, cx| dashboard.show_progress(&id, cx))
                }),
                on_follow_up: Rc::new(move |window: &mut Window, cx: &mut App| {
                    new_workspace_dialog::open_with(
                        &dashboard,
                        repo.clone(),
                        Some(title.clone()),
                        window,
                        cx,
                    );
                }),
            }
            .into_any_element();
        }
        let dashboard = self.dashboard.read(cx);
        let parent_branch = task
            .parent_id
            .as_deref()
            .and_then(|id| dashboard.data.task(id))
            .and_then(|parent| parent.branch.clone());
        let waiting_for = waits_for(&task, &dashboard.data.tasks);
        let preparing = dashboard.data.preparing.get(&task.id).copied();
        let agent = task
            .agent
            .clone()
            .filter(|agent| is_agent_id(Some(agent)))
            .unwrap_or_else(|| DEFAULT_AGENT.to_owned());
        let this = cx.weak_entity();
        NoWorkspace {
            task,
            branches: shown.branches.clone(),
            base: shown.base.clone(),
            parent_branch,
            busy: shown.starting,
            waiting_for,
            preparing,
            on_pick: Rc::new({
                let this = this.clone();
                move |value: String, _: &mut Window, cx: &mut App| {
                    this.update(cx, |view, cx| {
                        if let Some(shown) = view.shown.as_mut() {
                            shown.base = Some(value);
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }),
            on_create: Rc::new({
                let this = this.clone();
                move |_: &mut Window, cx: &mut App| {
                    this.update(cx, |view, cx| view.start(None, cx)).ok();
                }
            }),
            on_start: Rc::new(move |_: &mut Window, cx: &mut App| {
                this.update(cx, |view, cx| view.start(Some(agent.clone()), cx))
                    .ok();
            }),
        }
        .into_any_element()
    }

    pub(super) fn finished_actions(
        &self,
        task: &Task,
        cx: &mut Context<Self>,
    ) -> (FinishedActions, WrapUpActions) {
        let this = cx.weak_entity();
        let push = Rc::new({
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                this.update(cx, |view, cx| view.git_step(GitStep::Push, cx))
                    .ok();
            }
        });
        let pull_request = Rc::new({
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                this.update(cx, |view, cx| view.git_step(GitStep::PullRequest, cx))
                    .ok();
            }
        });
        let (commit, view_changes) = self.changes_actions(&task.id);
        let dashboard = self.dashboard.clone();
        let removed = task.clone();
        let banner = FinishedActions {
            push: push.clone(),
            pull_request: pull_request.clone(),
            commit,
            view_changes: view_changes.clone(),
            remove: Rc::new(move |window: &mut Window, cx: &mut App| {
                remove_workspace_dialog::open(&dashboard, removed.clone(), window, cx)
            }),
        };
        let wrap_up = WrapUpActions {
            push,
            pull_request,
            view_changes,
            reopen: Rc::new({
                let this = this.clone();
                move |_: &mut Window, cx: &mut App| {
                    this.update(cx, |view, cx| view.reopen(cx)).ok();
                }
            }),
            open_terminals: Rc::new({
                let this = this.clone();
                move |_: &mut Window, cx: &mut App| {
                    this.update(cx, |view, cx| view.show_terminals(cx)).ok();
                }
            }),
            stop: Rc::new({
                let this = this.clone();
                move |name: &str, window: &mut Window, cx: &mut App| {
                    this.update(cx, |view, cx| view.stop(name, window, cx)).ok();
                }
            }),
            toggle_screen: Rc::new(move |name: &str, _: &mut Window, cx: &mut App| {
                let name = name.to_owned();
                this.update(cx, |view, cx| {
                    if let Some(shown) = view.shown.as_mut() {
                        shown.shown_screen = if shown.shown_screen.as_deref() == Some(name.as_str())
                        {
                            None
                        } else {
                            Some(name)
                        };
                    }
                    cx.notify();
                })
                .ok();
            }),
        };
        (banner, wrap_up)
    }

    /// Commit… and View changes: the task's Changes tab; Commit… also drafts into an empty box
    /// and focuses it.
    fn changes_actions(&self, task_id: &str) -> (OnClick, OnClick) {
        let open = |commit: bool| -> OnClick {
            let (dashboard, id) = (self.dashboard.clone(), task_id.to_owned());
            Rc::new(move |_: &mut Window, cx: &mut App| {
                dashboard.update(cx, |dashboard, cx| dashboard.open_changes(&id, commit, cx))
            })
        };
        (open(true), open(false))
    }

    /// The git buttons of the tab bar end: Commit… or Push, and Create PR or Open PR.
    fn git_buttons(
        &self,
        task: &Task,
        shown: &Shown,
        uncommitted: Option<u64>,
        icon_only: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let this = cx.weak_entity();
        GitButtons::new(
            task.clone(),
            shown.graph.clone(),
            uncommitted,
            shown.pending,
        )
        .on_push({
            let this = this.clone();
            move |_, cx| {
                this.update(cx, |view, cx| view.git_step(GitStep::Push, cx))
                    .ok();
            }
        })
        .on_pull_request(move |_, cx| {
            this.update(cx, |view, cx| view.git_step(GitStep::PullRequest, cx))
                .ok();
        })
        .on_commit({
            let (dashboard, id) = (self.dashboard.clone(), task.id.clone());
            move |_, cx| dashboard.update(cx, |dashboard, cx| dashboard.open_changes(&id, true, cx))
        })
        .render(icon_only)
    }

    /// The History tab: the graph of the task against its base branch.
    fn history(&self, task: &Task, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let dashboard = self.dashboard.clone();
        HistoryView {
            id: task.id.clone(),
            graph: shown.graph.clone(),
            error: shown.graph_error.clone(),
            uncommitted: shown.changes.as_ref().map(Vec::len).unwrap_or(0),
            narrow: self.width.read(cx).is_some_and(|width| width < 640.),
            on_show_changes: Rc::new(move |_: &mut Window, cx: &mut App| {
                dashboard.update(cx, |dashboard, cx| {
                    dashboard.pick_tab(WorkspaceTab::Changes, cx)
                })
            }),
            on_refresh: Rc::new(move |_: &mut Window, cx: &mut App| {
                this.update(cx, |view, cx| view.read_git_fresh(cx)).ok();
            }),
        }
        .into_any_element()
    }

    /// A task with a workspace: finished banner, tab row, and the body of its tab: the
    /// panes or the wrap-up, Changes, History, Docs, or the open file.
    pub(super) fn render_workspace(
        &mut self,
        task: Task,
        shown: &Shown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let finished = is_finished_state(task.state);
        let panes_shown = !finished || shown.shells_wanted;
        let facts = GitFacts {
            uncommitted: shown.changes.as_ref().map(|changes| changes.len() as u64),
            graph: shown.graph.clone(),
        };
        let paths: Vec<String> = shown
            .changes
            .iter()
            .flatten()
            .map(|change| change.path.clone())
            .collect();
        let icon_only = self
            .width
            .read(cx)
            .is_none_or(|width| width < ICON_ONLY_BELOW);
        let git_buttons = self.git_buttons(&task, shown, facts.uncommitted, icon_only, cx);
        let (banner_actions, wrap_up_actions) = self.finished_actions(&task, cx);
        let banner = finished.then(|| {
            FinishedBanner::new(task.clone(), facts.clone(), shown.pending, banner_actions)
        });
        let tab = self.dashboard.read(cx).shown_workspace_tab();
        if !matches!(tab, WorkspaceTab::Docs | WorkspaceTab::Editor) {
            self.file_tab_shown = None;
        }
        let uncommitted = shown.changes.as_ref().map(Vec::len);
        let tab_row = self.tab_row(&task, tab, panes_shown, uncommitted, git_buttons, cx);
        let body: AnyElement = match tab {
            WorkspaceTab::Changes => self.changes.clone().into_any_element(),
            WorkspaceTab::History => self.history(&task, shown, cx),
            WorkspaceTab::Docs => self.docs_body(&task.id, window, cx),
            WorkspaceTab::Editor => self.editor_body(&task.id, window, cx),
            _ if panes_shown => self.panes.clone().into_any_element(),
            _ => WrapUp {
                task: task.clone(),
                facts,
                paths,
                list: self
                    .dashboard
                    .read(cx)
                    .data
                    .terminals
                    .get(&task.id)
                    .cloned(),
                ended: shown.ended.clone(),
                shown: shown.shown_screen.clone(),
                reopening: shown.reopening,
                pending: shown.pending,
                actions: wrap_up_actions,
            }
            .into_any_element(),
        };
        v_flex()
            .relative()
            .size_full()
            .on_action(cx.listener(Self::new_terminal))
            .child(width_probe(&self.width))
            .children(banner)
            .child(tab_row)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}
