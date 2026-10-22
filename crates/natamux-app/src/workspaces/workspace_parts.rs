//! The parts of the workspace view: the tab row, the no-workspace view and the finished
//! summary, the workspace itself, and what the finished banner's and the wrap-up's buttons do.

use std::rc::Rc;

use gpui::ClickEvent;
use natamux_proto::tasks::task::Task;
use ui::{Tooltip, prelude::*};

use super::{Shown, WorkspaceView};
use crate::agents::agent_choices::{DEFAULT_AGENT, is_agent_id};
use crate::tasks::state_filter::width_probe;
use crate::tasks::table_rules::waits_for;
use crate::terminals::pane_header::icon_button;
use crate::terminals::pane_rules::MAX_PANES;
use crate::workspaces::finished_banner::{FinishedActions, FinishedBanner};
use crate::workspaces::finished_summary::{FinishedSummary, follow_up_title};
use crate::workspaces::no_workspace::NoWorkspace;
use crate::workspaces::workspace_header::WorkspaceHeader;
use crate::workspaces::workspace_rules::GitStep;
use crate::workspaces::wrap_up::{WrapUp, WrapUpActions};
use crate::workspaces::wrap_up_rules::{GitFacts, is_finished_state};
use crate::workspaces::{new_workspace_dialog, remove_workspace_dialog};

impl WorkspaceView {
    pub(super) fn tab_row(
        &self,
        task: &Task,
        panes_shown: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let full = self.panes.read(cx).is_full(cx);
        let new_terminal = panes_shown.then(|| {
            let mut tip = "New terminal (Ctrl+Shift+`)".to_owned();
            if task.agent.is_some() {
                tip.push_str(". Terminal 1 runs the agent.");
            }
            let tip = if full {
                format!("At most {MAX_PANES} terminals")
            } else {
                tip
            };
            icon_button("tab-new-terminal", "square-split-horizontal")
                .disabled(full)
                .tooltip(Tooltip::text(tip))
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    view.panes.update(cx, |panes, cx| panes.add(cx))
                }))
                .child(Label::new("New terminal").size(LabelSize::Small))
        });
        let back = (is_finished_state(task.state) && panes_shown).then(|| {
            Button::new("tab-back-to-wrap-up", "Back to wrap-up")
                .label_size(LabelSize::Small)
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    if let Some(shown) = view.shown.as_mut() {
                        shown.shells_wanted = false;
                    }
                    cx.notify();
                }))
        });
        h_flex()
            .h(px(36.))
            .flex_none()
            .px_2()
            .gap_2()
            .border_b_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .h_full()
                    .px_2()
                    .gap_1p5()
                    .border_b_2()
                    .border_color(colors.text_accent)
                    .child(
                        Icon::from_path("icons/lucide/terminal.svg")
                            .size(IconSize::Small)
                            .color(Color::Default),
                    )
                    .child(Label::new("Terminal").size(LabelSize::Small)),
            )
            .child(div().flex_1())
            .children(back)
            .children(new_terminal)
    }

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
        let dashboard = self.dashboard.clone();
        let removed = task.clone();
        let banner = FinishedActions {
            push: push.clone(),
            pull_request: pull_request.clone(),
            remove: Rc::new(move |window: &mut Window, cx: &mut App| {
                remove_workspace_dialog::open(&dashboard, removed.clone(), window, cx)
            }),
        };
        let wrap_up = WrapUpActions {
            push,
            pull_request,
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

    /// A task with a workspace: header, finished banner, tab row, and its panes or wrap-up.
    pub(super) fn render_workspace(
        &self,
        task: Task,
        shown: &Shown,
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
        let this = cx.weak_entity();
        let header = WorkspaceHeader::new(
            task.clone(),
            shown.graph.clone(),
            facts.uncommitted,
            shown.pending,
            *self.width.read(cx),
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
        .on_progress({
            let (dashboard, id) = (self.dashboard.clone(), task.id.clone());
            move |_, cx| dashboard.update(cx, |dashboard, cx| dashboard.show_progress(&id, cx))
        });
        let (banner_actions, wrap_up_actions) = self.finished_actions(&task, cx);
        let banner = finished.then(|| {
            FinishedBanner::new(task.clone(), facts.clone(), shown.pending, banner_actions)
        });
        let tab_row = self.tab_row(&task, panes_shown, cx);
        let body: AnyElement = if panes_shown {
            self.panes.clone().into_any_element()
        } else {
            WrapUp {
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
            .into_any_element()
        };
        v_flex()
            .relative()
            .size_full()
            .on_action(cx.listener(Self::new_terminal))
            .child(width_probe(&self.width))
            .child(header)
            .children(banner)
            .child(tab_row)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}
