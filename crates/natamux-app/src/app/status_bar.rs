//! The status bar of docs/ui-spec.md 4.28 (`features/app/StatusBar.tsx`), lean as Zed's: at the
//! left the sidebar and agents toggles. At the right: need you and running as an icon and a count,
//! the background jobs with their menu, the link dot (its word only while down), then the details
//! toggle, as Zed puts the right dock's toggle at the right. The account is in the title bar and the
//! notify switch in its menu. Under 1024 jobs lose their label (`status_layout`).

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight};
use natamux_proto::tasks::job::Job;
use ui::{ButtonLike, ContextMenu, Tooltip, prelude::*};

use super::jobs::{job_rows, jobs_label};
use super::shell_rules::{LiveItem, needs_you_text, running_text};
use super::status_items::{OnClick, count_item, item_label, toggle_item};
use super::status_layout::forced_by_width;
use crate::components::colors::signal;
use crate::components::menu::MenuLayer;
use crate::components::spinner::Spinner;

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
    on_toggle_sidebar: OnClick,
    on_toggle_details: OnClick,
    on_toggle_agents: OnClick,
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
            on_toggle_sidebar: Rc::new(|_, _, _| {}),
            on_toggle_details: Rc::new(|_, _, _| {}),
            on_toggle_agents: Rc::new(|_, _, _| {}),
            sidebar: false,
            details: Ok(false),
            agents: false,
        }
    }

    /// The states of the three toggles: sidebar and agents at the left, details at the right.
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

    /// The clicks of the sidebar, details and agents toggles.
    pub fn on_toggles(
        mut self,
        sidebar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        details: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        agents: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle_sidebar = Rc::new(sidebar);
        self.on_toggle_details = Rc::new(details);
        self.on_toggle_agents = Rc::new(agents);
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

fn on(shown: bool, hide: &'static str, show: &'static str) -> &'static str {
    if shown { hide } else { show }
}

/// The two toggles at the left: sidebar, agents.
fn left_toggles(
    sidebar: bool,
    agents: bool,
    on_sidebar: OnClick,
    on_agents: OnClick,
) -> impl IntoElement {
    h_flex()
        .gap(rems_from_px(2.0_f32))
        .child(toggle_item(
            "toggle-sidebar",
            "panel-left",
            on(sidebar, "Hide sidebar", "Show sidebar"),
            "sidebar.toggle",
            sidebar,
            None,
            on_sidebar,
        ))
        .child(toggle_item(
            "toggle-agents",
            "layout-grid",
            on(agents, "Hide agents", "Show agents"),
            "agents.toggle",
            agents,
            None,
            on_agents,
        ))
}

/// The details toggle, last at the right: it opens the right dock.
fn details_toggle(details: Result<bool, &'static str>, on_click: OnClick) -> impl IntoElement {
    let shown = details == Ok(true);
    toggle_item(
        "toggle-details",
        "panel-right",
        on(shown, "Hide details", "Show details"),
        "details.toggle",
        shown,
        details.err(),
        on_click,
    )
}

/// The connection: a dot in the link's colour, and its word only while the link is down.
fn live_item(live: LiveItem, cx: &App) -> impl IntoElement {
    let dot = if live.ok {
        cx.theme().status().success
    } else {
        signal(cx).fg
    };
    let tip: SharedString = live.tooltip().into();
    let label = live.shown_label();
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
        .when_some(label, |this, label| {
            this.child(item_label(label, Color::Muted))
        })
}

/// The background jobs with their menu; none while no job runs.
fn jobs_item(jobs: Vec<Job>, open: OnOpenTask, icon_only: bool) -> Option<ButtonLike> {
    let label = short_jobs_label(&jobs)?;
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
                    .child(Spinner::new(IconSize::Small))
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
        let signal_fg = Color::Custom(signal(cx).fg);
        let needs = (self.needs_you > 0).then(|| {
            let count = Label::new(self.needs_you.to_string())
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(signal_fg);
            let tip = needs_you_text(self.needs_you);
            count_item("status-needs-you", "circle-alert", signal_fg, count, tip)
                .on_click(move |event, window, cx| (self.on_needs_you)(event, window, cx))
        });
        let running = (self.running > 0).then(|| {
            let count = item_label(self.running.to_string(), Color::Muted);
            let tip = running_text(self.running);
            count_item("status-running", "circle-play", Color::Muted, count, tip)
                .on_click(move |event, window, cx| (self.on_running)(event, window, cx))
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
            .child(left_toggles(
                self.sidebar,
                self.agents,
                self.on_toggle_sidebar,
                self.on_toggle_agents,
            ))
            .child(div().flex_1())
            .children(needs)
            .children(running)
            .children(jobs_item(
                self.jobs,
                self.on_open_task,
                condensed.icon_only.contains(&"jobs"),
            ))
            .child(live_item(self.live, cx))
            .child(details_toggle(self.details, self.on_toggle_details))
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
