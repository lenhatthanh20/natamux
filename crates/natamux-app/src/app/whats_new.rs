//! The What's new dialog (T-0301): the built-in release notes of the running version. It opens by
//! itself once after an update, and from the menu, the palette and Settings at any time.

use std::time::Duration;

use gpui::{DismissEvent, EventEmitter, FocusHandle, Focusable, WindowHandle};
use ui::prelude::*;

use super::links::open_link;
use super::update::updates_folder;
use super::view_state;
use super::whats_new_rules::{CHANGELOG_URL, NOTES_FILE, VERSION, dialog_text, show_on_start};
use super::window::Root;
use crate::components::button_style::{action_button, quiet_button};
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView};
use crate::components::docs_markdown::MarkdownDoc;

/// The window restores its tasks first, so the dialog does not open into a moving layout.
const AFTER_START: Duration = Duration::from_secs(1);
const BODY_MAX_H: f32 = 520.;

pub fn open(window: &mut Window, cx: &mut App) {
    open_notes(NOTES_FILE, window, cx);
}

/// `notes`: a release notes file. The shot passes fixed notes, so it stays the same each release.
pub fn open_notes(notes: &str, window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| WhatsNew::new(notes, cx));
    DialogLayer::open(view, window, cx);
}

/// After the window opens: shows the notes when this start follows an update, and saves the
/// version so they show once. Not for fake, smoke or `--shot` runs (the caller checks).
pub fn on_start(handle: WindowHandle<Root>, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(AFTER_START).await;
        let _ = handle.update(cx, |root, window, cx| {
            let dashboard = root.dashboard().clone();
            let seen = dashboard.read(cx).view().whats_new_seen.clone();
            if seen.as_deref() == Some(VERSION) {
                return;
            }
            let updated_here = view_state::folder()
                .is_some_and(|folder| updates_folder(&folder, VERSION).is_dir());
            let show = show_on_start(seen.as_deref(), VERSION, updated_here);
            // Another dialog is open: try again on the next start, so no dialog waits in a queue.
            if show && DialogLayer::is_open(cx) {
                return;
            }
            dashboard.update(cx, |dashboard, cx| dashboard.set_whats_new_seen(VERSION, cx));
            if show {
                open(window, cx);
            }
        });
    })
    .detach();
}

pub struct WhatsNew {
    notes: MarkdownDoc,
    focus: FocusHandle,
}

impl WhatsNew {
    fn new(text: &str, cx: &mut Context<Self>) -> Self {
        let notes = MarkdownDoc::new(cx);
        notes.set_text(&dialog_text(text), cx);
        Self {
            notes,
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<DismissEvent> for WhatsNew {}

impl Focusable for WhatsNew {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for WhatsNew {
    fn size(&self) -> DialogSize {
        DialogSize::Large
    }

    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        vec![self.focus.clone()]
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for WhatsNew {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let done = action_button("whats-new-close", "Close")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)));
        let changelog = quiet_button("whats-new-changelog", "Full changelog")
            .on_click(|_, _, cx| open_link(CHANGELOG_URL, cx));
        div().track_focus(&self.focus).child(
            DialogFrame::new(format!("What's new in Natamux {VERSION}"), close)
                .body_max_h(BODY_MAX_H)
                .footer(changelog)
                .footer(done)
                .child(self.notes.body(window, cx)),
        )
    }
}
