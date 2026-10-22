//! The sidebar (docs/ui-spec.md 3.1, `Sidebar.tsx`): the filter field (`/`), the All tasks row
//! with its count, the "Workspaces" header with Start a task, and the tree, which is the only part
//! that scrolls. The shell (`app/window.rs`) sets its width, its resize handle and the drawer.

// The spec names the view after its folder.
#![allow(clippy::module_inception)]

use editor::{Editor, EditorEvent};
use gpui::{ClickEvent, Entity, FocusHandle, Focusable, FontWeight, Stateful, Subscription};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{Tooltip, prelude::*};

use super::workspace_tree::WorkspaceTree;
use crate::agents::agent_choices::agent_label;
use crate::app::commands::shortcut_of;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::Link;
use crate::app::shortcuts::Back;
use crate::components::agent_badge::AgentIcon;
use crate::components::chips::{Kbd, tip_with_keys};
use crate::components::colors::state_colors;
use crate::components::skeleton::{Loading, SkeletonList};
use crate::sidebar::sidebar_text::{pull_request_number, pull_request_summary};
use crate::terminals::pane_rules::agent_pane_tasks;
use crate::workspaces::new_workspace_dialog;

pub struct Sidebar {
    dashboard: Entity<Dashboard>,
    filter: Entity<Editor>,
    tree: Entity<WorkspaceTree>,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Filter tasks", window, cx);
            editor
        });
        let edits = cx.subscribe(&filter, |sidebar, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                let text = editor.read(cx).text(cx);
                sidebar
                    .dashboard
                    .update(cx, |dashboard, cx| dashboard.set_filter(text, cx));
            }
        });
        // Clear filter in main's empty state changes the dashboard's filter: the field follows.
        let observe = cx.observe_in(&dashboard, window, |sidebar, dashboard, window, cx| {
            let wanted = dashboard.read(cx).filter.clone();
            if sidebar.filter.read(cx).text(cx) != wanted {
                sidebar
                    .filter
                    .update(cx, |editor, cx| editor.set_text(wanted, window, cx));
            }
            cx.notify();
        });
        let tree = cx.new(|cx| WorkspaceTree::new(dashboard.clone(), cx));
        Sidebar {
            dashboard,
            filter,
            tree,
            _subscriptions: vec![edits, observe],
        }
    }

    /// The filter field, for `--smoke`.
    pub fn filter(&self) -> Entity<Editor> {
        self.filter.clone()
    }

    /// The tree, for `--gate2` and for focus moves of the shell.
    pub fn tree(&self) -> &Entity<WorkspaceTree> {
        &self.tree
    }

    /// Esc in the field clears it, then leaves it for the tree, where the single keys work.
    fn escape(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.read(cx).text(cx).is_empty() {
            let tree = self.tree.focus_handle(cx);
            window.focus(&tree, cx);
        } else {
            self.filter
                .update(cx, |editor, cx| editor.clear(window, cx));
        }
    }

    fn filter_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let empty = self.filter.read(cx).text(cx).is_empty();
        let clear = (!empty).then(|| {
            IconButton::new("filter-clear", IconName::Close)
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text("Clear the filter"))
                .on_click(cx.listener(|sidebar, _: &ClickEvent, window, cx| {
                    sidebar
                        .filter
                        .update(cx, |editor, cx| editor.clear(window, cx));
                }))
        });
        let hint = empty.then(|| Kbd::new(shortcut_of("filter.focus").unwrap_or("/")));
        h_flex()
            .h(px(28.))
            .px_2()
            .gap(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.editor_background)
            .on_action(cx.listener(Self::escape))
            .child(lucide("list-filter").color(Color::Muted))
            .child(div().min_w_0().flex_1().child(self.filter.clone()))
            .children(clear)
            .children(hint)
    }

    fn all_tasks_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dashboard = self.dashboard.read(cx);
        let count = dashboard.data.tasks.len();
        let active = dashboard.open_task.is_none() && !dashboard.agents_open;
        nav_row("nav-all-tasks", "list-todo", "All tasks", count, active, cx).on_click(cx.listener(
            |sidebar, _: &ClickEvent, _, cx| {
                sidebar
                    .dashboard
                    .update(cx, |dashboard, cx| dashboard.back(cx));
            },
        ))
    }

    /// The Agents grid (4.23): the panes of the tasks whose agents work or wait.
    fn agents_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dashboard = self.dashboard.read(cx);
        let view = dashboard.view();
        let count = agent_pane_tasks(view.agent_panes.as_deref(), &dashboard.data.tasks).len();
        let active = dashboard.agents_open;
        nav_row("nav-agents", "layout-grid", "Agents", count, active, cx).on_click(cx.listener(
            |sidebar, _: &ClickEvent, _, cx| {
                sidebar
                    .dashboard
                    .update(cx, |dashboard, cx| dashboard.open_agents(cx));
            },
        ))
    }

    fn workspaces_header(&self) -> impl IntoElement {
        let keys = shortcut_of("workspace.new").unwrap_or_default();
        let dashboard = self.dashboard.clone();
        h_flex()
            .h(px(28.))
            .pl_2()
            .pr(px(2.))
            .child(
                div().flex_1().child(
                    Label::new("WORKSPACES")
                        .size(LabelSize::Custom(rems_from_px(11_f32)))
                        .weight(FontWeight::SEMIBOLD)
                        .color(Color::Muted),
                ),
            )
            .child(
                IconButton::new("workspaces-start", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(tip_with_keys("Start a task", keys))
                    .on_click(move |_, window, cx| {
                        new_workspace_dialog::open(&dashboard, None, window, cx);
                    }),
            )
    }
}

impl Focusable for Sidebar {
    /// The drawer gives the focus to the tree.
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.tree.focus_handle(cx)
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = &self.dashboard.read(cx).data;
        let loading = !data.loaded && data.link == Link::Loading;
        let no_projects = data.loaded
            && data.repos.is_empty()
            && !data.tasks.iter().any(|task| task.workspace.is_some());
        let tree = Loading::new("sidebar-loading", loading, SkeletonList::new(8, 40.))
            .content(self.tree.clone());
        let none = no_projects.then(|| div().py_1().pl_2().child(small("No projects yet")));
        v_flex()
            .size_full()
            .gap_2()
            .pt_2()
            .child(
                v_flex().gap_2().px_2().child(self.filter_field(cx)).child(
                    v_flex()
                        .child(self.all_tasks_row(cx))
                        .child(self.agents_row(cx)),
                ),
            )
            .child(
                v_flex()
                    .min_h_0()
                    .flex_1()
                    .child(div().px_2().child(self.workspaces_header()))
                    .child(div().min_h_0().flex_1().child(tree))
                    .children(none),
            )
    }
}

/// A 28 px nav row of the sidebar: All tasks and Agents, with their counts.
fn nav_row(
    id: &'static str,
    icon: &str,
    label: &'static str,
    count: usize,
    active: bool,
    cx: &App,
) -> Stateful<Div> {
    let colors = cx.theme().colors();
    h_flex()
        .id(id)
        .h(px(28.))
        .px_2()
        .gap(px(6.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|style| style.bg(colors.element_hover))
        .when(active, |row| row.bg(colors.element_selected))
        .child(lucide(icon).size(IconSize::Small).color(Color::Muted))
        .child(
            div().flex_1().child(
                Label::new(label)
                    .size(LabelSize::Custom(rems_from_px(13_f32)))
                    .color(Color::Muted),
            ),
        )
        .child(small(count.to_string()))
}

// Text and icons the sidebar's rows share.

/// What a blurred project shows in place of private text (names, titles, ids, questions).
fn masked(text: &str) -> String {
    "•".repeat(text.chars().count().clamp(3, 12))
}

pub(super) fn private(text: &str, blurred: bool) -> SharedString {
    if blurred {
        masked(text).into()
    } else {
        text.to_owned().into()
    }
}

pub(super) fn small(text: impl Into<SharedString>) -> Label {
    Label::new(text)
        .size(LabelSize::Custom(rems_from_px(12_f32)))
        .color(Color::Muted)
        .single_line()
}

pub(super) fn mono(text: impl Into<SharedString>, cx: &App) -> Label {
    Label::new(text)
        .size(LabelSize::Custom(rems_from_px(11_f32)))
        .color(Color::Muted)
        .buffer_font(cx)
        .single_line()
}

pub(super) fn lucide(name: &str) -> Icon {
    Icon::from_path(format!("icons/lucide/{name}.svg")).size(IconSize::XSmall)
}

/// The agent of a task or a terminal, as a small mark with its name in the tooltip.
pub(super) fn agent_mark(id: ElementId, agent: &str) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .child(AgentIcon::new(Some(agent.to_owned())))
        .tooltip(Tooltip::text(agent_label(Some(agent))))
}

/// The colour of a pull request number: green when checks pass, red when they fail.
fn checks_color(task: &Task) -> Color {
    match task.pull_request_status().map(|status| status.checks) {
        Some(checks) if checks == "passing" => Color::Success,
        Some(checks) if checks == "failing" => Color::Error,
        Some(checks) if checks == "pending" => Color::Warning,
        _ => Color::Muted,
    }
}

/// The pull request number in the colour of its checks.
pub(super) fn pull_request_mark(task: &Task) -> Option<Stateful<Div>> {
    let url = task.pr.as_deref()?;
    let color = checks_color(task);
    let summary = pull_request_summary(task.pull_request_status().as_ref());
    let tip = format!("Pull request {}", summary.unwrap_or_default());
    Some(
        h_flex()
            .id(SharedString::from(format!("pr-{}", task.id)))
            .flex_none()
            .gap_0p5()
            .child(lucide("git-pull-request").color(color))
            .child(small(pull_request_number(url)).color(color))
            .tooltip(Tooltip::text(tip.trim().to_owned())),
    )
}

/// The 6 px dot of a state, such as the one after the count of a folded group.
pub(super) fn state_dot(state: TaskState, cx: &App) -> impl IntoElement {
    let color = state_colors(state, cx).fg;
    div().flex_none().size(px(6.)).rounded_full().bg(color)
}
