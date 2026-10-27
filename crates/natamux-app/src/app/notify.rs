//! The needs-you notifications (docs/ui-spec.md 4.12, `useTaskNotifications.ts`): when a task
//! starts to need the user, or becomes done, while the app window is not active, a system
//! notification names it and the alert sound plays. A click brings the app forward and opens the
//! task. macOS shows none when the app does not run from its bundle, as in a dev run. The Dock
//! icon shows the needs-you count. The screen-reader announcement text is kept too; GPUI has no
//! live region to speak it yet (`docs/rust-phase3.md`).

use gpui::{AnyWindowHandle, Entity, SystemNotification};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::dashboard::Dashboard;
use super::dock_badge::set_dock_badge;
use super::notify_rules::{
    NeedsYouAnnouncer, States, badge_label, plays_sound, rings, states_by_id,
};
use crate::tasks::task_attention::needs_me_tasks;

#[derive(Default)]
pub struct Notifier {
    /// None before the first list: opening the app does not ring.
    previous: Option<States>,
    pub announcer: NeedsYouAnnouncer,
    /// The Dock label last set; the Dock starts with none.
    badge: Option<String>,
}

impl Notifier {
    /// After each task list: the Dock count, and a ring for what changed while the window is not
    /// active. `window` is the main window, which plays the sound.
    pub fn after_tasks(&mut self, dashboard: &Entity<Dashboard>, window: &Window, cx: &mut App) {
        let board = dashboard.read(cx);
        let tasks = board.data.tasks.clone();
        let repos = board.data.repos.clone();
        let (on, loaded) = (board.view().notify, board.data.loaded);
        let now = super::clock::now_ms();
        let waiting: Vec<Task> = needs_me_tasks(&tasks, now);
        self.announcer.update(&waiting, loaded);
        self.set_badge(badge_label(waiting.len()));
        let Some(previous) = self.previous.replace(states_by_id(&tasks)) else {
            return;
        };
        if window.is_window_active() || !on {
            return;
        }
        let ring = rings(&previous, &tasks, &repos);
        for (id, note) in &ring {
            cx.show_system_notification(SystemNotification {
                tag: id.clone().into(),
                title: note.title.clone().into(),
                body: note.body.clone().into(),
                actions: Vec::new(),
            });
        }
        if plays_sound(&ring) {
            window.play_system_bell();
        }
    }

    fn set_badge(&mut self, label: Option<String>) {
        if self.badge != label {
            set_dock_badge(label.as_deref());
            self.badge = label;
        }
    }
}

/// A click on a notification brings the main window forward and opens the task it names. A note
/// of a task that is gone, or of no task, opens nothing.
pub fn follow_clicks(dashboard: &Entity<Dashboard>, main: AnyWindowHandle, cx: &App) {
    let dashboard = dashboard.downgrade();
    cx.on_system_notification_response(move |response, cx| {
        let Some(dashboard) = dashboard.upgrade() else {
            return;
        };
        let id = response.tag.to_string();
        main.update(cx, |_, window, cx| {
            window.activate_window();
            dashboard.update(cx, |dashboard, cx| dashboard.open(&id, cx));
        })
        .ok();
    });
}
