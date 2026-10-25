//! The needs-you notifications (docs/ui-spec.md 4.12, `useTaskNotifications.ts`): when a task
//! starts to need the user, or becomes done, while the app window is not active, a small pop-up
//! window at the top right of the screen names it. A click brings the app forward and opens the
//! task. GPUI has no system notification API and the app is unsigned, so the pop-up is the app's
//! own window, as Zed's agent notification is. The screen-reader announcement text is kept too;
//! GPUI has no live region to speak it yet (`docs/rust-phase3.md`).

use std::time::Duration;

use gpui::{
    AnyWindowHandle, Bounds, Entity, FontWeight, Size, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowKind, WindowOptions, point,
};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::dashboard::Dashboard;
use super::notify_rules::{
    NeedsYouAnnouncer, Notification, States, notification_for, states_by_id, tasks_that_finished,
    tasks_that_started_waiting,
};
use crate::tasks::task_attention::needs_me_tasks;

/// Zed's agent notification: 450 x 72, 16 from the right edge.
const SIZE: (f32, f32) = (450., 72.);
const MARGIN: f32 = 16.;
/// The pop-up goes by itself after this.
const SHOWN_FOR: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct Notifier {
    /// None before the first list: opening the app does not ring.
    previous: Option<States>,
    pub announcer: NeedsYouAnnouncer,
    shown: Vec<AnyWindowHandle>,
}

impl Notifier {
    /// After each task list: ring for what changed while the window is not active.
    pub fn after_tasks(
        &mut self,
        dashboard: &Entity<Dashboard>,
        main: AnyWindowHandle,
        active: bool,
        cx: &mut App,
    ) {
        let board = dashboard.read(cx);
        let tasks = board.data.tasks.clone();
        let (on, loaded) = (board.view().notify, board.data.loaded);
        let now = super::clock::now_ms();
        let waiting: Vec<Task> = needs_me_tasks(&tasks, now);
        self.announcer.update(&waiting, loaded);
        let Some(previous) = self.previous.replace(states_by_id(&tasks)) else {
            return;
        };
        if active || !on {
            return;
        }
        let ring: Vec<(String, Notification)> = tasks_that_started_waiting(&previous, &tasks)
            .into_iter()
            .chain(tasks_that_finished(&previous, &tasks))
            .map(|task| (task.id.clone(), notification_for(task)))
            .collect();
        for (id, note) in ring {
            self.show(id, note, dashboard, main, cx);
        }
    }

    fn show(
        &mut self,
        task_id: String,
        note: Notification,
        dashboard: &Entity<Dashboard>,
        main: AnyWindowHandle,
        cx: &mut App,
    ) {
        let Some(screen) = cx.primary_display() else {
            return;
        };
        let size = Size {
            width: px(SIZE.0),
            height: px(SIZE.1),
        };
        // One under the other when several ring at once; closed ones leave room.
        self.shown
            .retain(|window| window.update(cx, |_, _, _| ()).is_ok());
        let row = self.shown.len() as f32;
        let origin = screen.bounds().top_right()
            - point(size.width + px(MARGIN), px(-MARGIN - row * (SIZE.1 + 8.)));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size })),
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            display_id: Some(screen.id()),
            window_background: WindowBackgroundAppearance::Opaque,
            window_decorations: Some(WindowDecorations::Client),
            ..Default::default()
        };
        let dashboard = dashboard.clone();
        let opened = cx.open_window(options, |_, cx| {
            cx.new(|_| NotifyView {
                note,
                task_id,
                dashboard,
                main,
            })
        });
        let Ok(handle) = opened else {
            return;
        };
        let any: AnyWindowHandle = handle.into();
        self.shown.push(any);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(SHOWN_FOR).await;
            any.update(cx, |_, window, _| window.remove_window()).ok();
        })
        .detach();
    }
}

struct NotifyView {
    note: Notification,
    task_id: String,
    dashboard: Entity<Dashboard>,
    main: AnyWindowHandle,
}

impl Render for NotifyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .id("notification")
            .size_full()
            .px_4()
            .py_3()
            .gap_1()
            .bg(colors.elevated_surface_background)
            .border_1()
            .border_color(colors.border)
            .rounded_lg()
            .cursor_pointer()
            .on_click(cx.listener(|view, _, window, cx| {
                let (id, dashboard) = (view.task_id.clone(), view.dashboard.clone());
                view.main
                    .update(cx, |_, main, cx| {
                        main.activate_window();
                        dashboard.update(cx, |dashboard, cx| dashboard.open(&id, cx));
                    })
                    .ok();
                window.remove_window();
            }))
            .child(Label::new(self.note.title.clone()).weight(FontWeight::SEMIBOLD))
            .child(
                Label::new(self.note.body.clone())
                    .color(Color::Muted)
                    .truncate(),
            )
    }
}
