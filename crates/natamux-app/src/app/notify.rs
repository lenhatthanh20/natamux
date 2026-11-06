//! The needs-you notifications (docs/ui-spec.md 4.12, `useTaskNotifications.ts`): when a task
//! starts to need the user, or becomes done, while the app window is not active, a system
//! notification names it, with the terminal that waits, and the alert sound plays. A click brings
//! the app forward and opens the task on that terminal. macOS shows none when the app does not
//! run from its bundle, as in a dev run. The Dock icon shows the needs-you count once macOS
//! allows badges (`dock_badge.rs`). The screen-reader announcement text is kept too; GPUI has no live region to speak it yet (`docs/rust-phase3.md`).

use gpui::{AnyWindowHandle, Entity, SystemNotification};
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use ui::prelude::*;

use super::dashboard::Dashboard;
use super::dock_badge::set_dock_badge;
use super::notify_rules::{
    NeedsYouAnnouncer, Notification, Ring, States, badge_label, note_tag, plays_sound, rings,
    states_by_id, tag_target, unread_turn_ends, with_terminal,
};
use crate::tasks::task_attention::needs_me_tasks;
use crate::tasks::task_rules::needs_me;

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
        let always = board.view().notify_always;
        let now = super::clock::now_ms();
        let waiting: Vec<Task> = needs_me_tasks(&tasks, now);
        self.announcer.update(&waiting, loaded);
        let unread = unread_turn_ends(&tasks, &board.view().seen);
        self.set_badge(badge_label(waiting.len() + unread));
        let Some(previous) = self.previous.replace(states_by_id(&tasks)) else {
            return;
        };
        if !on || (window.is_window_active() && !always) {
            return;
        }
        let ring = rings(&previous, &tasks, &repos);
        for each in &ring {
            post_naming_terminal(dashboard, each, cx);
        }
        if plays_sound(&ring) {
            window.play_system_bell();
        }
    }

    /// After the user opened a task: an unread turn end it had is read now.
    pub fn after_open(&mut self, dashboard: &Entity<Dashboard>, cx: &App) {
        let board = dashboard.read(cx);
        let waiting = needs_me_tasks(&board.data.tasks, super::clock::now_ms()).len();
        let unread = unread_turn_ends(&board.data.tasks, &board.view().seen);
        self.set_badge(badge_label(waiting + unread));
    }

    fn set_badge(&mut self, label: Option<String>) {
        if self.badge != label {
            set_dock_badge(label.as_deref());
            self.badge = label;
        }
    }
}

fn post(note: &Notification, tag: String, cx: &mut App) {
    cx.show_system_notification(SystemNotification {
        tag: tag.into(),
        title: note.title.clone().into(),
        body: note.body.clone().into(),
        actions: Vec::new(),
    });
}

/// The terminal that works on `stage`, the one that waits first when several do.
fn terminal_of(list: &TerminalList, stage: &str) -> Option<String> {
    let mut on_stage: Vec<_> = list
        .lanes
        .iter()
        .filter(|(_, lane)| lane.stage.as_deref() == Some(stage))
        .collect();
    on_stage.sort_by_key(|(_, lane)| !lane.state.is_some_and(needs_me));
    on_stage.first().map(|(name, _)| (*name).clone())
}

/// Posts the note with the terminal of its stage. The app keeps the terminals only of the tasks
/// it shows, so for another task it reads them first; without them the note names the task only.
fn post_naming_terminal(dashboard: &Entity<Dashboard>, ring: &Ring, cx: &mut App) {
    let Some(stage) = ring.stage.clone() else {
        post(&ring.note, note_tag(&ring.task_id, None), cx);
        return;
    };
    let finish = {
        let (note, task_id) = (ring.note.clone(), ring.task_id.clone());
        move |terminal: Option<String>, cx: &mut App| {
            let note = match &terminal {
                Some(terminal) => with_terminal(&note, terminal),
                None => note,
            };
            post(&note, note_tag(&task_id, terminal.as_deref()), cx);
        }
    };
    // A kept list that does not show the wait yet is old: an ended terminal may name the stage.
    let known = dashboard.read(cx).data.terminals.get(&ring.task_id);
    let fresh = known.filter(|list| {
        list.lanes.values().any(|lane| {
            lane.stage.as_deref() == Some(stage.as_str()) && lane.state.is_some_and(needs_me)
        })
    });
    if let Some(terminal) = fresh.and_then(|list| terminal_of(list, &stage)) {
        finish(Some(terminal), cx);
        return;
    }
    let read = dashboard.update(cx, |dashboard, cx| {
        dashboard.read_terminals(&ring.task_id, cx)
    });
    cx.spawn(async move |cx| {
        let terminal = read.await.ok().and_then(|list| terminal_of(&list, &stage));
        cx.update(|cx| finish(terminal, cx));
    })
    .detach();
}

/// A click on a notification brings the main window forward and opens the task it names, on the
/// terminal it names. A note of a task that is gone, or of no task, opens nothing.
pub fn follow_clicks(dashboard: &Entity<Dashboard>, main: AnyWindowHandle, cx: &App) {
    let dashboard = dashboard.downgrade();
    cx.on_system_notification_response(move |response, cx| {
        let Some(dashboard) = dashboard.upgrade() else {
            return;
        };
        let tag = response.tag.to_string();
        let (id, terminal) = tag_target(&tag);
        main.update(cx, |_, window, cx| {
            window.activate_window();
            dashboard.update(cx, |dashboard, cx| match terminal {
                Some(terminal) => dashboard.open_shell(id, Some(terminal.to_owned()), cx),
                None => dashboard.open(id, cx),
            });
        })
        .ok();
    });
}
