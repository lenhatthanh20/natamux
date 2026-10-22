//! The Agents grid (docs/ui-spec.md 4.23, `features/terminals/AgentPanes.tsx`): Terminal 1 of
//! many tasks, the same terminal as in each task's workspace. By default the tasks whose agents
//! work or wait; once the user adds or closes a pane, their own list (kept in `view.json`).
//! Closing a pane leaves its agent running.

use std::collections::BTreeSet;
use std::rc::Rc;

use gpui::{ClickEvent, Entity, Focusable, FontWeight, Pixels, Point, Subscription};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::TerminalList;
use ui::{ButtonLike, ContextMenu, Tooltip, prelude::*};

use super::pane_facts::{agent_lanes, restoring_label};
use super::pane_header::{PaneHeader, PaneTitle};
use super::pane_rules::{
    MAX_PANES, PANE_LAYOUTS, PaneLayout, agent_pane_tasks, columns_for, max_columns,
    other_terminal_hint, pane_state_text,
};
use super::shell_panes::grid_rows;
use super::terminal_cache::{Owner, TerminalCache, terminal_key};
use super::terminal_find::open_find;
use super::terminal_pane::TerminalPane;
use crate::app::dashboard::{Dashboard, TerminalReader};
use crate::components::chips::CountBadge;
use crate::components::empty_state::EmptyState;
use crate::components::menu::MenuLayer;
use crate::sidebar::shell_rows::terminal_state;
use crate::tasks::state_filter::width_probe;
use crate::workspaces::wrap_up_rules::is_finished_state;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// The tasks "Add a pane" offers: a workspace, not finished, not shown yet.
pub fn pane_candidates<'a>(tasks: &'a [Task], shown: &[String]) -> Vec<&'a Task> {
    tasks
        .iter()
        .filter(|task| task.workspace.is_some() && !is_finished_state(task.state))
        .filter(|task| !shown.contains(&task.id))
        .collect()
}

/// The user's list after a pane is added or closed. The first change starts from the default
/// list, which the grid showed.
pub fn with_pane(shown: &[String], id: &str, add: bool) -> Vec<String> {
    let mut ids: Vec<String> = shown.iter().filter(|each| *each != id).cloned().collect();
    if add && ids.len() < MAX_PANES {
        ids.push(id.to_owned());
    }
    ids
}

fn layout_label(layout: PaneLayout) -> &'static str {
    match layout {
        PaneLayout::One => "1",
        PaneLayout::Two => "2",
        PaneLayout::Four => "4",
        PaneLayout::Auto => "Auto",
    }
}

fn layout_columns(layout: PaneLayout) -> Option<usize> {
    match layout {
        PaneLayout::One => Some(1),
        PaneLayout::Two => Some(2),
        PaneLayout::Four => Some(4),
        PaneLayout::Auto => None,
    }
}

pub struct AgentPanes {
    dashboard: Entity<Dashboard>,
    maximized: Option<String>,
    width: Entity<Option<f32>>,
    _width: Subscription,
}

impl AgentPanes {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|_| None);
        let observe = cx.observe(&width, |_, _, cx| cx.notify());
        AgentPanes {
            dashboard,
            maximized: None,
            width,
            _width: observe,
        }
    }

    fn shown_ids(&self, cx: &App) -> Vec<String> {
        let dashboard = self.dashboard.read(cx);
        agent_pane_tasks(
            dashboard.view().agent_panes.as_deref(),
            &dashboard.data.tasks,
        )
        .into_iter()
        .map(|task| task.id.clone())
        .collect()
    }

    fn change(&mut self, id: &str, add: bool, cx: &mut Context<Self>) {
        let ids = with_pane(&self.shown_ids(cx), id, add);
        if self.maximized.as_deref() == Some(id) {
            self.maximized = None;
        }
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_agent_panes(Some(ids), cx));
    }

    fn add_menu(&self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.shown_ids(cx);
        let candidates: Vec<(String, String)> =
            pane_candidates(&self.dashboard.read(cx).data.tasks, &shown)
                .into_iter()
                .map(|task| (task.id.clone(), task.title.clone()))
                .collect();
        let this = cx.weak_entity();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for (id, title) in &candidates {
                let (this, id) = (this.clone(), id.clone());
                menu = menu.entry(format!("{title}  {}", id), None, move |_, cx| {
                    this.update(cx, |panes, cx| panes.change(&id, true, cx))
                        .ok();
                });
            }
            menu
        });
        MenuLayer::open(menu, position, window, cx);
    }

    fn header(
        &self,
        count: usize,
        can_add: bool,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let layout = self.dashboard.read(cx).view().pane_layout;
        let fits = max_columns(width);
        let add = can_add.then(|| {
            ButtonLike::new("agents-add-pane")
                .style(ButtonStyle::Outlined)
                .on_click(cx.listener(|panes, event: &ClickEvent, window, cx| {
                    panes.add_menu(event.position(), window, cx)
                }))
                .child(
                    h_flex()
                        .gap_1p5()
                        .px_1()
                        .child(Icon::from_path("icons/lucide/plus.svg").size(IconSize::Small))
                        .child(Label::new("Add a pane").size(LabelSize::Small)),
                )
        });
        let columns = h_flex().gap_0p5().children(PANE_LAYOUTS.iter().map(|each| {
            let each = *each;
            let too_wide = layout_columns(each).is_some_and(|columns| columns > fits);
            let button = ButtonLike::new(SharedString::from(format!(
                "agents-columns-{}",
                each.as_str()
            )))
            .style(ButtonStyle::Subtle)
            .toggle_state(each == layout)
            .disabled(too_wide)
            .on_click(cx.listener(move |panes, _: &ClickEvent, _, cx| {
                panes
                    .dashboard
                    .update(cx, |dashboard, cx| dashboard.set_pane_layout(each, cx))
            }))
            .child(
                div()
                    .px_1()
                    .child(Label::new(layout_label(each)).size(LabelSize::Small)),
            );
            if too_wide {
                button.tooltip(Tooltip::text("Needs a wider window"))
            } else {
                button
            }
        }));
        h_flex()
            .h(px(44.))
            .flex_none()
            .px_4()
            .gap_2()
            .border_b_1()
            .border_color(colors.border)
            .child(
                Label::new("Agents")
                    .size(LabelSize::Large)
                    .weight(FontWeight::SEMIBOLD),
            )
            .child(CountBadge::new(count))
            .child(div().flex_1())
            .children(add)
            .child(columns)
    }

    /// Maximize or restore one task's pane.
    fn toggle_of(
        &self,
        id: &str,
        cx: &Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) + Clone + 'static {
        let (this, id) = (cx.weak_entity(), id.to_owned());
        move |_: &mut Window, cx: &mut App| {
            this.update(cx, |panes, cx| {
                panes.maximized = if panes.maximized.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    Some(id.clone())
                };
                cx.notify();
            })
            .ok();
        }
    }

    /// The header of a task's pane: its Terminal 1 lane, else the task, always with the mark.
    fn header_of(&self, task: &Task, list: &TerminalList, cx: &Context<Self>) -> PaneHeader {
        let activity = self
            .dashboard
            .read(cx)
            .data
            .activity
            .get(&task.id)
            .and_then(|lines| lines.get("1"))
            .map(|line| line.text.clone());
        let lanes = agent_lanes(list);
        let lane = lanes.get("1");
        let state = lane
            .and_then(|lane| terminal_state(task, lane))
            .unwrap_or(task.state);
        let agent = lane
            .and_then(|lane| lane.agent.clone().flatten())
            .or_else(|| task.agent.clone());
        let id = task.id.clone();
        let open: OnClick = Rc::new({
            let (dashboard, id) = (self.dashboard.clone(), id.clone());
            move |_: &mut Window, cx: &mut App| {
                dashboard.update(cx, |dashboard, cx| dashboard.open(&id, cx))
            }
        });
        let maximized = self.maximized.as_deref() == Some(id.as_str());
        let this = cx.weak_entity();
        let title = PaneTitle::Task {
            id: id.clone(),
            title: task.title.clone(),
            open: open.clone(),
        };
        let text = pane_state_text(state, activity.as_deref());
        PaneHeader::new(terminal_key(&id, "1"), title)
            .agent_shell(Some(state), agent)
            .state_text(Some((text, state == TaskState::NeedsInput)))
            .hint(other_terminal_hint(task, &lanes))
            .maximize(maximized, self.toggle_of(&id, cx))
            .open_workspace(move |window, cx| open(window, cx))
            .close(
                "Remove from Agents (the agent keeps running)",
                move |_, cx| {
                    this.update(cx, |panes, cx| panes.change(&id, false, cx))
                        .ok();
                },
            )
    }

    fn pane(&self, task: &Task, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let source = self.dashboard.read(cx).source().cloned()?;
        let list = self
            .dashboard
            .read(cx)
            .data
            .terminals
            .get(&task.id)
            .cloned()
            .unwrap_or_default();
        let header = self.header_of(task, &list, cx);
        let key = terminal_key(&task.id, "1");
        let terminal = TerminalCache::terminal(&source, &task.id, "1", window, cx);
        let focused = terminal
            .read(cx)
            .view
            .focus_handle(cx)
            .contains_focused(window, cx);
        let find = TerminalCache::find(&key, cx);
        let this = cx.weak_entity();
        Some(
            TerminalPane::new(key.clone(), terminal.clone(), header)
                .focused(focused)
                .restoring(restoring_label(&list, "1"))
                .find(find, move |window, cx| {
                    this.update(cx, |_, cx| {
                        open_find(key.clone(), terminal.clone(), window, cx)
                    })
                    .ok();
                })
                .on_maximize(self.toggle_of(&task.id, cx))
                .into_any_element(),
        )
    }
}

impl Render for AgentPanes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (tasks, candidates, layout) = {
            let dashboard = self.dashboard.read(cx);
            let shown: Vec<Task> = agent_pane_tasks(
                dashboard.view().agent_panes.as_deref(),
                &dashboard.data.tasks,
            )
            .into_iter()
            .cloned()
            .collect();
            let ids: Vec<String> = shown.iter().map(|task| task.id.clone()).collect();
            let candidates = pane_candidates(&dashboard.data.tasks, &ids).len();
            dashboard.show_terminals(TerminalReader::Agents, ids);
            (shown, candidates, dashboard.view().pane_layout)
        };
        let width = *self.width.read(cx);
        let can_add = candidates > 0 && tasks.len() < MAX_PANES;
        let header = self.header(tasks.len(), can_add, width, cx);
        let visible: Vec<&Task> = match &self.maximized {
            Some(id) => tasks.iter().filter(|task| task.id == *id).collect(),
            None => tasks.iter().collect(),
        };
        let keys: BTreeSet<String> = visible
            .iter()
            .map(|task| terminal_key(&task.id, "1"))
            .collect();
        TerminalCache::claim(Owner::Agents, keys, cx);
        let body: AnyElement = if visible.is_empty() {
            let mut empty = EmptyState::new("agents-empty", "layout-grid", "No agent is working")
                .body("Start an agent on a task, or add a pane.");
            if can_add {
                empty = empty.action(
                    Button::new("agents-empty-add", "Add a pane")
                        .style(ButtonStyle::Filled)
                        .on_click(cx.listener(|panes, event: &ClickEvent, window, cx| {
                            panes.add_menu(event.position(), window, cx)
                        })),
                );
            }
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(empty)
                .into_any_element()
        } else {
            let panes: Vec<AnyElement> = visible
                .iter()
                .filter_map(|task| self.pane(task, window, cx))
                .collect();
            let columns = if self.maximized.is_some() {
                1
            } else {
                columns_for(layout, panes.len(), width)
            };
            v_flex()
                .id("agent-panes")
                .size_full()
                .p_2()
                .gap_2()
                .overflow_y_scroll()
                .children(grid_rows(panes, columns))
                .into_any_element()
        };
        v_flex()
            .relative()
            .size_full()
            .child(width_probe(&self.width))
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, state: TaskState, workspace: bool) -> Task {
        Task {
            id: id.into(),
            state,
            workspace: workspace.then(|| "/w".into()),
            ..Task::default()
        }
    }

    #[test]
    fn offers_the_open_workspaces_that_are_not_shown() {
        let tasks = [
            task("T-1", TaskState::Running, true),
            task("T-2", TaskState::Review, true),
            task("T-3", TaskState::Done, true),
            task("T-4", TaskState::Ready, false),
        ];
        let offered: Vec<&str> = pane_candidates(&tasks, &["T-1".into()])
            .iter()
            .map(|task| task.id.as_str())
            .collect();
        assert_eq!(offered, ["T-2"]);
    }

    #[test]
    fn adds_a_pane_at_the_end_and_closes_one_in_place() {
        let shown: Vec<String> = ["T-1", "T-2"].map(String::from).to_vec();
        assert_eq!(with_pane(&shown, "T-3", true), ["T-1", "T-2", "T-3"]);
        assert_eq!(with_pane(&shown, "T-1", false), ["T-2"]);
        let full: Vec<String> = (1..=6).map(|n| format!("T-{n}")).collect();
        assert_eq!(with_pane(&full, "T-7", true).len(), MAX_PANES);
    }
}
