//! The status bar of docs/ui-spec.md 4.28 (`features/app/StatusBar.tsx`): at the left the sidebar,
//! details and agents toggles. At the right: "N need you", "N running", the background jobs with
//! their menu, Live, then the notify switch, the keyboard and the account (`status_items`). Under
//! 1024 jobs and Live lose their labels and running hides (`status_layout`).

use std::rc::Rc;

use gpui::{ClickEvent, Entity, FontWeight};
use natamux_proto::tasks::job::Job;
use ui::{ButtonLike, ContextMenu, Tooltip, prelude::*};

use super::dashboard::Dashboard;
use super::jobs::{job_rows, jobs_label};
use super::shell_rules::{LiveItem, needs_you_text};
use super::shortcuts::{ToggleAgents, ToggleDetails, ToggleSidebar};
use super::status_items::{
    OnClick, account_item, item_button, item_label, keyboard_item, toggle_item,
};
use super::status_layout::forced_by_width;
use crate::components::colors::signal;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

pub const STATUS_BAR_HEIGHT: f32 = 28.;

type OnOpenTask = Rc<dyn Fn(&str, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct StatusBar {
    live: LiveItem,
    running: usize,
    needs_you: usize,
    /// The daemon's background jobs, oldest first.
    jobs: Vec<Job>,
    width: f32,
    on_running: OnClick,
    on_needs_you: OnClick,
    on_open_task: OnOpenTask,
    notify: bool,
    on_notify: OnClick,
    account: Option<Entity<Dashboard>>,
    sidebar: bool,
    /// Whether the details show, or why the toggle cannot show them (`details_toggle`).
    details: Result<bool, &'static str>,
    agents: bool,
}

impl StatusBar {
    pub fn new(
        live: LiveItem,
        running: usize,
        needs_you: usize,
        jobs: Vec<Job>,
        width: f32,
    ) -> Self {
        Self {
            live,
            running,
            needs_you,
            jobs,
            width,
            on_running: Rc::new(|_, _, _| {}),
            on_needs_you: Rc::new(|_, _, _| {}),
            on_open_task: Rc::new(|_, _, _| {}),
            notify: false,
            on_notify: Rc::new(|_, _, _| {}),
            account: None,
            sidebar: false,
            details: Ok(false),
            agents: false,
        }
    }

    /// The states of the three toggles at the left.
    pub fn toggles(
        mut self,
        sidebar: bool,
        details: Result<bool, &'static str>,
        agents: bool,
    ) -> Self {
        self.sidebar = sidebar;
        self.details = details;
        self.agents = agents;
        self
    }

    /// The dashboard the account menu changes. Without it the account item is left out.
    pub fn account(mut self, dashboard: Entity<Dashboard>) -> Self {
        self.account = Some(dashboard);
        self
    }

    /// "Notify me" / "Notifications on": the app's own switch, kept in `view.json`.
    pub fn notify(
        mut self,
        on: bool,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.notify = on;
        self.on_notify = Rc::new(f);
        self
    }

    pub fn on_running(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_running = Rc::new(f);
        self
    }

    pub fn on_needs_you(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_needs_you = Rc::new(f);
        self
    }

    /// A row of the jobs menu opens its task.
    pub fn on_open_task(mut self, f: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_task = Rc::new(f);
        self
    }
}

pub use crate::app::clock::now_ms;

/// The jobs menu, opened upward from the bar: one row per job with its task and how long it has
/// run. A row without a task is not a link.
fn open_jobs_menu(
    jobs: &[Job],
    on_open_task: OnOpenTask,
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let rows = job_rows(jobs, now_ms());
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        for row in rows {
            let text = (row.label.clone(), row.hint.clone());
            let render = move |_: &mut Window, _: &mut App| {
                h_flex()
                    .w_full()
                    .gap_3()
                    .justify_between()
                    .child(Label::new(text.0.clone()))
                    .child(item_label(text.1.clone(), Color::Muted))
                    .into_any_element()
            };
            menu = match row.open_task {
                Some(task_id) => {
                    let open = on_open_task.clone();
                    menu.custom_entry(render, move |window, cx| open(&task_id, window, cx))
                }
                None => menu.custom_row(render),
            };
        }
        menu
    });
    MenuLayer::open(menu, event.position(), window, cx);
}

/// "N jobs" for two or more; one job keeps its own label.
fn short_jobs_label(jobs: &[Job]) -> Option<String> {
    match jobs {
        [] | [_] => jobs_label(jobs),
        many => Some(format!("{} jobs", many.len())),
    }
}

fn notify_item(on: bool, on_click: OnClick) -> ButtonLike {
    let (icon, tip) = if on {
        ("bell-off", "Stop the notifications of tasks that need you")
    } else {
        (
            "bell",
            "Notify me when a task needs me while the app is in the background",
        )
    };
    item_button("status-notify", tip, on_click).child(
        Icon::from_path(icon_path(icon))
            .size(IconSize::Small)
            .color(Color::Muted),
    )
}

/// The three toggles at the left: sidebar, details, agents.
fn toggles(sidebar: bool, details: Result<bool, &'static str>, agents: bool) -> impl IntoElement {
    let on = |shown: bool, hide: &'static str, show: &'static str| if shown { hide } else { show };
    let details_on = details == Ok(true);
    h_flex()
        .gap(rems_from_px(2.0_f32))
        .child(toggle_item(
            "toggle-sidebar",
            "panel-left",
            on(sidebar, "Hide sidebar", "Show sidebar"),
            "sidebar.toggle",
            sidebar,
            None,
            || Box::new(ToggleSidebar),
        ))
        .child(toggle_item(
            "toggle-details",
            "panel-right",
            on(details_on, "Hide details", "Show details"),
            "details.toggle",
            details_on,
            details.err(),
            || Box::new(ToggleDetails),
        ))
        .child(toggle_item(
            "toggle-agents",
            "layout-grid",
            on(agents, "Hide agents", "Show agents"),
            "agents.toggle",
            agents,
            None,
            || Box::new(ToggleAgents),
        ))
}

/// The connection: a dot in the link's colour, and its label unless the bar is narrow.
fn live_item(live: LiveItem, icon_only: bool, cx: &App) -> impl IntoElement {
    let dot = if live.ok {
        cx.theme().status().success
    } else {
        signal(cx).fg
    };
    let tip: SharedString = live.tip.clone().into();
    h_flex()
        .id("status-live")
        .h(rems_from_px(24.0_f32))
        .px(rems_from_px(6.0_f32))
        .gap(rems_from_px(6.0_f32))
        .rounded(rems_from_px(6.0_f32))
        .tooltip(Tooltip::text(tip))
        .child(
            div()
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .bg(dot),
        )
        .when(!icon_only, |this| {
            this.child(item_label(live.label, Color::Muted))
        })
}

/// The background jobs with their menu; none while no job runs.
fn jobs_item(jobs: Vec<Job>, open: OnOpenTask, icon_only: bool) -> Option<ButtonLike> {
    let label = short_jobs_label(&jobs)?;
    // A still icon, not a spinner: an endless animation redraws the window every frame.
    Some(
        ButtonLike::new("status-jobs")
            .height(rems_from_px(24.0_f32).into())
            .tooltip(Tooltip::text("Show background jobs"))
            .on_click(move |event, window, cx| {
                open_jobs_menu(&jobs, open.clone(), event, window, cx)
            })
            .child(
                h_flex()
                    .gap(rems_from_px(6.0_f32))
                    .child(
                        Icon::from_path(icon_path("loader-circle"))
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .when(!icon_only, |this| {
                        this.child(item_label(label, Color::Muted))
                    }),
            ),
    )
}

impl RenderOnce for StatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let condensed = forced_by_width(Some(self.width));
        let icon_only = |id: &str| condensed.icon_only.contains(&id);
        let folded = |id: &str| condensed.folded.contains(&id);
        let running =
            item_button("status-running", "Show the running tasks", self.on_running).child(
                item_label(format!("{} running", self.running), Color::Muted),
            );
        let needs = (self.needs_you > 0).then(|| {
            item_button(
                "status-needs-you",
                "Show the tasks that wait for you",
                self.on_needs_you,
            )
            .child(
                Label::new(needs_you_text(self.needs_you))
                    .size(LabelSize::Small)
                    .weight(FontWeight::MEDIUM)
                    .color(Color::Custom(signal(cx).fg)),
            )
        });
        h_flex()
            .flex_none()
            .w_full()
            .h(rems_from_px(STATUS_BAR_HEIGHT))
            .px(rems_from_px(6.0_f32))
            .gap(rems_from_px(2.0_f32))
            .overflow_hidden()
            .bg(colors.status_bar_background)
            .border_t_1()
            .border_color(colors.border_variant)
            .child(toggles(self.sidebar, self.details, self.agents))
            .child(div().flex_1())
            .children(needs)
            .when(!folded("running"), |this| this.child(running))
            .children(jobs_item(self.jobs, self.on_open_task, icon_only("jobs")))
            .child(live_item(self.live, icon_only("live"), cx))
            .child(notify_item(self.notify, self.on_notify.clone()))
            .child(keyboard_item())
            .children(self.account.map(account_item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(label: &str) -> Job {
        serde_json::from_value(serde_json::json!({
            "id": label, "kind": "push", "label": label, "taskId": null, "startedAt": "2026-10-21T09:00:00.000Z"
        }))
        .expect("a job")
    }

    #[test]
    fn says_n_jobs_for_two_or_more() {
        assert_eq!(short_jobs_label(&[]), None);
        assert_eq!(
            short_jobs_label(&[job("a"), job("b")]).as_deref(),
            Some("2 jobs")
        );
    }
}
