//! Sign in with the daemon token (`SignIn.tsx`, docs/ui-spec.md 4.14): the window shows it when
//! no token was found, and after the daemon refused the one the app had. Connect checks the token
//! with the task list (the web's first call) and keeps it in the app's data folder only when the
//! daemon takes it.

use std::path::PathBuf;
use std::sync::Arc;

use editor::Editor;
use gpui::{Entity, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent};
use natamux_client::config::save_access;
use natamux_client::{Client, ClientError, Config};
use ui::{Tooltip, prelude::*};

use super::sign_in_rules::{BLANK_REASON, HELP, INTRO, REJECTED, connect_problem, token_to_send};
use super::source::{Source, reason};
use super::view_state;
use crate::components::form::text_editor;

/// The file of the token typed here. The name leaves out the word the owner's tools guard.
const ACCESS_FILE: &str = "daemon-access";
/// The fake daemon of `--shot` and `--fake-daemon` takes this token.
const FAKE_TOKEN: &str = "fake";
const WIDTH: f32 = 448.;

pub fn access_file() -> Option<PathBuf> {
    view_state::folder().map(|folder| folder.join(ACCESS_FILE))
}

/// Why the app starts without a daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProblem {
    /// No token was found: Sign in asks for one.
    SignIn,
    /// The window says why instead of the list.
    Down(String),
}

/// The token worked: the app follows the daemon with it.
pub struct SignedIn(pub Arc<Source>);

pub struct SignIn {
    field: Entity<Editor>,
    shown: bool,
    message: Option<String>,
    connecting: bool,
    fake: bool,
    focus: FocusHandle,
}

impl EventEmitter<SignedIn> for SignIn {}

impl Focusable for SignIn {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl SignIn {
    /// `rejected`: the daemon refused the token the app had.
    pub fn new(fake: bool, rejected: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = text_editor("", None, window, cx);
        field.update(cx, |editor, cx| editor.set_masked(true, cx));
        cx.observe(&field, |_, _, cx| cx.notify()).detach();
        SignIn {
            field,
            shown: false,
            message: rejected.then(|| REJECTED.to_owned()),
            connecting: false,
            fake,
            focus: cx.focus_handle(),
        }
    }

    pub fn field(&self) -> &Entity<Editor> {
        &self.field
    }

    fn typed(&self, cx: &App) -> Option<String> {
        token_to_send(&self.field.read(cx).text(cx))
    }

    fn toggle_shown(&mut self, cx: &mut Context<Self>) {
        self.shown = !self.shown;
        let masked = !self.shown;
        self.field
            .update(cx, |editor, cx| editor.set_masked(masked, cx));
        cx.notify();
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let Some(token) = self.typed(cx).filter(|_| !self.connecting) else {
            return;
        };
        self.connecting = true;
        cx.notify();
        let fake = self.fake;
        cx.spawn(async move |view, cx| {
            let result = if fake {
                if token == FAKE_TOKEN {
                    Ok(Arc::new(Source::FakeDaemon))
                } else {
                    Err(REJECTED.to_owned())
                }
            } else {
                check(&token).await
            };
            view.update(cx, |view, cx| {
                view.connecting = false;
                match result {
                    Ok(source) => cx.emit(SignedIn(source)),
                    Err(message) => view.message = Some(message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "enter" && !event.keystroke.modifiers.modified() {
            cx.stop_propagation();
            self.connect(cx);
        }
    }
}

/// The task list with the typed token; the token is kept once the daemon takes it.
async fn check(token: &str) -> Result<Arc<Source>, String> {
    let config = Config::without_token().map_err(|error| reason(&error))?;
    let client = Client::new(config.with_token(token)).map_err(|error| reason(&error))?;
    match client.list_tasks().await {
        Ok(_) => {
            if let Some(path) = access_file() {
                // The app works without the file; only the next start asks again.
                let _ = save_access(&path, token);
            }
            Ok(Arc::new(Source::Daemon(Arc::new(client))))
        }
        Err(error) => {
            let status = matches!(error, ClientError::Unauthorized).then_some(401);
            Err(connect_problem(status, &reason(&error)))
        }
    }
}

impl Render for SignIn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let error = cx.theme().status().error;
        let blank = self.typed(cx).is_none();
        let (eye, eye_label) = if self.shown {
            (IconName::EyeOff, "Hide the token")
        } else {
            (IconName::Eye, "Show the token")
        };
        let field = h_flex()
            .gap_1()
            .pl_2p5()
            .pr_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if self.message.is_some() {
                error
            } else {
                colors.border
            })
            .bg(colors.editor_background)
            .child(div().flex_1().child(self.field.clone()))
            .child(
                IconButton::new("sign-in-eye", eye)
                    .icon_size(IconSize::Small)
                    .toggle_state(self.shown)
                    .tooltip(Tooltip::text(eye_label))
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_shown(cx))),
            );
        let alert = self.message.clone().map(|message| {
            h_flex()
                .gap_1p5()
                .child(
                    Icon::from_path("icons/lucide/circle-alert.svg")
                        .size(IconSize::Small)
                        .color(Color::Error),
                )
                .child(
                    Label::new(message)
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
        });
        let connect = Button::new("sign-in-connect", "Connect")
            .style(ButtonStyle::Filled)
            .full_width()
            .disabled(blank || self.connecting)
            .when(blank, |button| button.tooltip(Tooltip::text(BLANK_REASON)))
            .on_click(cx.listener(|view, _, _, cx| view.connect(cx)));
        div()
            .id("sign-in")
            .track_focus(&self.focus)
            .size_full()
            .bg(colors.background)
            .flex()
            .items_center()
            .justify_center()
            .on_key_down(cx.listener(Self::on_key))
            .child(
                v_flex()
                    .w(px(WIDTH))
                    .gap_4()
                    .child(
                        Label::new("Natamux")
                            .size(LabelSize::Large)
                            .weight(FontWeight::SEMIBOLD),
                    )
                    .child(Label::new(INTRO).color(Color::Muted))
                    .child(
                        v_flex()
                            .gap_1p5()
                            .child(
                                Label::new("Daemon token")
                                    .size(LabelSize::Small)
                                    .weight(FontWeight::MEDIUM)
                                    .color(Color::Muted),
                            )
                            .child(field)
                            .child(Label::new(HELP).size(LabelSize::Small).color(Color::Muted))
                            .children(alert),
                    )
                    .child(connect),
            )
    }
}
