//! Repo trust: the dialog that shows every command a project runs on this machine, word for word
//! with where it comes from, and its setup scripts. Approve sends the hash it showed; a 409 means
//! the commands changed, so it reads and shows the new ones. Not now, Esc and the close button
//! leave the project untrusted; the daemon then runs none of its commands.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{DismissEvent, EventEmitter, FocusHandle, Focusable};
use natamux_client::ClientError;
use natamux_proto::projects::repo_trust::{RepoCommand, RepoTrustView, SetupScript, TRUST_NOTE};
use ui::prelude::*;

use crate::app::source::{Source, reason};
use crate::components::button_style::button_icon;
use crate::components::colors::TileColor;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogSize, DialogView, cancel_button, primary_button,
};
use crate::components::settings_card::{
    SeparatorInset, SettingsCard, SettingsGroup, error_line, foot_text,
};
use crate::components::settings_tile::{AlertKind, SettingsAlert, SettingsTile, TileSize};

#[path = "repo_trust_row.rs"]
pub mod row;
#[path = "repo_trust_rules.rs"]
pub mod rules;

use rules::{CHANGED, LEAD, needs_review, title};

/// What runs once the dialog closes, whichever the answer: Start a task goes on with it.
pub type Then = Rc<dyn Fn(&mut Window, &mut App)>;

/// Reads the project's commands and opens the dialog over the open one when they wait for
/// approval. `then` runs once it closes, or at once when there is nothing to approve or the read
/// fails (the daemon still blocks the commands).
pub fn review<V: 'static>(
    source: &Arc<Source>,
    repo_id: &str,
    name: &str,
    then: Option<Then>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let answer = source.repo_trust(repo_id);
    let (source, repo_id, name) = (source.clone(), repo_id.to_owned(), name.to_owned());
    cx.spawn_in(window, async move |this, cx| {
        let view = answer.await.ok().filter(needs_review);
        if this.upgrade().is_none() {
            return;
        }
        // Not inside an update of `this`: `then` may update it (Start a task does), and a second
        // update of an entity that is being updated panics.
        cx.update(|window, cx| match view {
            Some(view) => open(source, repo_id, name, view, then, window, cx),
            None => go_on(then, window, cx),
        })
        .ok();
    })
    .detach();
}

fn go_on(then: Option<Then>, window: &mut Window, cx: &mut App) {
    if let Some(then) = then {
        then(window, cx);
    }
}

fn open(
    source: Arc<Source>,
    repo_id: String,
    name: String,
    view: RepoTrustView,
    then: Option<Then>,
    window: &mut Window,
    cx: &mut App,
) {
    let dialog = cx.new(|cx| RepoTrustDialog {
        source,
        repo_id,
        name,
        view,
        then,
        busy: false,
        error: None,
        focus: cx.focus_handle(),
        not_now: cx.focus_handle(),
        approve: cx.focus_handle(),
    });
    // The safe answer has the focus: Enter must not approve commands unread.
    let first = dialog.read(cx).not_now.clone();
    DialogLayer::open_over(dialog, window, cx);
    window.focus(&first, cx);
}

pub struct RepoTrustDialog {
    source: Arc<Source>,
    repo_id: String,
    name: String,
    view: RepoTrustView,
    then: Option<Then>,
    busy: bool,
    error: Option<String>,
    focus: FocusHandle,
    not_now: FocusHandle,
    approve: FocusHandle,
}

impl RepoTrustDialog {
    fn approve(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let answer = self.source.approve_repo(&self.repo_id, &self.view.hash);
        let (source, repo_id) = (self.source.clone(), self.repo_id.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = answer.await;
            let fresh = match &result {
                Err(error) if error.is_conflict() => Some(source.repo_trust(&repo_id).await),
                _ => None,
            };
            this.update_in(cx, |this, window, cx| {
                this.answered(result, fresh, window, cx)
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// `fresh`: the commands read again after a 409.
    fn answered(
        &mut self,
        result: Result<RepoTrustView, ClientError>,
        fresh: Option<Result<RepoTrustView, ClientError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        match (result, fresh) {
            (Ok(_), _) => self.close(window, cx),
            // Approved meanwhile, from another window or the CLI.
            (_, Some(Ok(view))) if !needs_review(&view) => self.close(window, cx),
            (_, Some(Ok(view))) => {
                self.view = view;
                self.error = Some(CHANGED.to_owned());
            }
            (Err(error), _) => self.error = Some(reason(&error)),
        }
        cx.notify();
    }

    /// Closes first, so a dialog that `then` opens is not closed with this one.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
        go_on(self.then.take(), window, cx);
    }
}

impl EventEmitter<DismissEvent> for RepoTrustDialog {}

impl Focusable for RepoTrustDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for RepoTrustDialog {
    fn size(&self) -> DialogSize {
        DialogSize::Large
    }

    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        vec![self.not_now.clone(), self.approve.clone()]
    }

    fn busy(&self, _: &App) -> bool {
        self.busy
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.approve(window, cx);
    }

    /// Esc and a click outside are Not now: Start a task still goes on.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.close(window, cx);
        true
    }
}

/// Text that runs, as it is: the buffer font in a box.
fn code_box(text: &str, cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .px(rems_from_px(10.0_f32))
        .py(rems_from_px(6.0_f32))
        .rounded(rems_from_px(6.0_f32))
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.editor_background)
        .child(
            Label::new(text.to_owned())
                .size(LabelSize::Small)
                .buffer_font(cx),
        )
}

/// A block of the Commands card: where it comes from, then the text that runs.
fn command_block(place: String, code: impl IntoElement) -> impl IntoElement {
    v_flex()
        .w_full()
        .px(rems_from_px(12.0_f32))
        .py(rems_from_px(10.0_f32))
        .child(div().mb(rems_from_px(4.0_f32)).child(foot_text(place)))
        .child(code)
}

fn command_row(command: &RepoCommand, cx: &App) -> impl IntoElement {
    command_block(command.place.clone(), code_box(&command.text, cx))
}

/// A script can be long: its box scrolls on its own.
fn script_box(index: usize, script: &SetupScript, cx: &App) -> impl IntoElement {
    let code = div()
        .id(("repo-trust-script", index))
        .max_h(rems_from_px(200.0_f32))
        .overflow_y_scroll()
        .child(code_box(&script.text, cx));
    command_block(format!("Setup script {}", script.file), code)
}

impl RepoTrustDialog {
    /// The commands, then the setup scripts, in one card.
    fn commands(&self, cx: &App) -> SettingsCard {
        let mut card = SettingsCard::new(SeparatorInset::Text);
        for command in &self.view.commands {
            card = card.child(command_row(command, cx));
        }
        for (index, script) in self.view.setup.iter().enumerate() {
            card = card.child(script_box(index, script, cx));
        }
        card
    }
}

impl Render for RepoTrustDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A 409 shows as a warning over the new commands; any other refusal under them.
        let changed = self.error.as_deref() == Some(CHANGED);
        let alert = changed.then(|| SettingsAlert::new(AlertKind::Warning, CHANGED));
        let failed = self.error.clone().filter(|_| !changed).map(error_line);
        let commands = SettingsGroup::new("Commands")
            .child(self.commands(cx))
            .footnote(TRUST_NOTE);
        let close = cx.listener(|this, _, window, cx| this.close(window, cx));
        let not_now = cx.listener(|this, _, window, cx| this.close(window, cx));
        let approve = cx.listener(|this, _, window, cx| this.approve(window, cx));
        let lead = SettingsTile::icon(button_icon("shield"), TileColor::Yellow, TileSize::Row);
        let approve = primary_button("Approve", &self.approve, self.busy, approve)
            .start_icon(button_icon("shield"));
        div().track_focus(&self.focus).child(
            DialogFrame::new(title(&self.name), close)
                .sheet()
                .lead(lead)
                .children(alert)
                .child(div().pt(rems_from_px(14.0_f32)).child(foot_text(LEAD)))
                .child(commands)
                .children(failed)
                .footer(cancel_button("Not now", &self.not_now, not_now))
                .footer(approve),
        )
    }
}

#[cfg(test)]
#[path = "repo_trust_dialog_tests.rs"]
mod tests;
