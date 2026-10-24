//! The status bar of docs/ui-spec.md 4.28 (`features/app/StatusBar.tsx`): Live or Reconnecting,
//! "N running", "N need you" and the background jobs with their menu. Under 1024 the dot and the
//! jobs lose their labels (`status_layout`). At the right: Projects, Import issues, the switch of
//! the needs-you notifications, the keyboard and the account (`status_items`). Under 1024
//! Projects, Import issues and the notifications fold into the "More status items" menu.

use std::rc::Rc;

use gpui::{ClickEvent, Entity, FontWeight};
use natamux_proto::tasks::job::Job;
use ui::{ButtonLike, ContextMenu, Tooltip, prelude::*};

use super::dashboard::Dashboard;
use super::jobs::{job_rows, jobs_label};
use super::shell_rules::{LiveItem, needs_you_text};
use super::status_items::{
    Folded, OnClick, account_item, import_item, item_button, item_label, keyboard_item, more_item,
    projects_item,
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
        }
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

fn notify_item(on: bool, on_click: OnClick) -> ButtonLike {
    let (icon, label, tip) = if on {
        (
            "bell-off",
            "Notifications on",
            "Stop the notifications of tasks that need you",
        )
    } else {
        (
            "bell",
            "Notify me",
            "Notify me when a task needs me while the app is in the background",
        )
    };
    item_button("status-notify", tip, on_click).child(
        h_flex()
            .gap(rems_from_px(6.0_f32))
            .child(
                Icon::from_path(icon_path(icon))
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(item_label(label, Color::Muted)),
    )
}

impl RenderOnce for StatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let condensed = forced_by_width(Some(self.width));
        let icon_only = |id: &str| condensed.icon_only.contains(&id);
        let folded = |id: &str| condensed.folded.contains(&id);
        let any_folded = !condensed.folded.is_empty();
        let dot = if self.live.ok {
            cx.theme().status().success
        } else {
            signal(cx).fg
        };
        let live_tip: SharedString = self.live.tip.clone().into();
        let live = h_flex()
            .id("status-live")
            .h(rems_from_px(24.0_f32))
            .px(rems_from_px(6.0_f32))
            .gap(rems_from_px(6.0_f32))
            .rounded(rems_from_px(6.0_f32))
            .tooltip(Tooltip::text(live_tip))
            .child(
                div()
                    .flex_none()
                    .size(rems_from_px(6.0_f32))
                    .rounded_full()
                    .bg(dot),
            )
            .when(!icon_only("live"), |this| {
                this.child(item_label(self.live.label, Color::Muted))
            });
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
        let jobs = jobs_label(&self.jobs).map(|label| {
            let (list, open) = (self.jobs, self.on_open_task);
            // A still icon, not a spinner: an endless animation redraws the window every frame.
            ButtonLike::new("status-jobs")
                .height(rems_from_px(24.0_f32).into())
                .tooltip(Tooltip::text("Show background jobs"))
                .on_click(move |event, window, cx| {
                    open_jobs_menu(&list, open.clone(), event, window, cx)
                })
                .child(
                    h_flex()
                        .gap(rems_from_px(6.0_f32))
                        .child(
                            Icon::from_path(icon_path("loader-circle"))
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .when(!icon_only("jobs"), |this| {
                            this.child(item_label(label, Color::Muted))
                        }),
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
            .child(live)
            .child(running)
            .children(needs)
            .children(jobs)
            .child(div().flex_1())
            .when(!folded("repos"), |this| {
                this.child(projects_item(icon_only("repos")))
            })
            .when(!folded("import"), |this| {
                this.child(import_item(icon_only("import")))
            })
            .when(!folded("notifications"), |this| {
                this.child(notify_item(self.notify, self.on_notify.clone()))
            })
            .when(any_folded, |this| {
                this.child(more_item(Folded {
                    repos: folded("repos"),
                    import: folded("import"),
                    notifications: folded("notifications")
                        .then(|| (self.notify, self.on_notify.clone())),
                }))
            })
            .child(keyboard_item())
            .children(self.account.map(account_item))
    }
}
