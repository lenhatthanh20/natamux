//! The Send back dialog (docs/ui-spec.md 4.14, `features/stages/SendBackDialog.tsx`): the stage to
//! go back to and what should change. Send back (Mod+Enter) makes `POST /tasks/:id/stage` with
//! `reopen`; the dialog closes only when the daemon took it.

use editor::Editor;
use gpui::{ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable};
use natamux_proto::stages::actions::{StageAction, StageActionInput};
use natamux_proto::stages::progress::StageProgress;
use natamux_proto::tasks::task::Task;
use ui::{ContextMenu, prelude::*};

use super::send_back_rules::{default_send_back_to, earlier_stages, send_back_reason};
use crate::app::dashboard::Dashboard;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::form::{field, input_box, text_editor};
use crate::components::menu::MenuLayer;

/// Opens the dialog for `stage` of `task`, with the stages it can go back to.
pub fn open(
    dashboard: &Entity<Dashboard>,
    task: &Task,
    stage: &str,
    progress: &[StageProgress],
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<SendBackDialog>> {
    let entry = progress.iter().find(|entry| entry.stage == stage)?;
    let earlier = earlier_stages(progress);
    let to = default_send_back_to(&earlier, entry).to_owned();
    let options: Vec<(String, String)> = earlier
        .iter()
        .map(|entry| (entry.stage.clone(), entry.name.clone()))
        .collect();
    let dashboard = dashboard.clone();
    let task_id = task.id.clone();
    let dialog = cx.new(|cx| {
        let reason = text_editor(
            "Add the error cases for an empty list.",
            Some((4, 8)),
            window,
            cx,
        );
        SendBackDialog {
            dashboard,
            task_id,
            options,
            to,
            reason,
            busy: false,
            focus: cx.focus_handle(),
            pick_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
        }
    });
    let field_focus = dialog.read(cx).reason.focus_handle(cx);
    DialogLayer::open(dialog.clone(), window, cx);
    window.focus(&field_focus, cx);
    Some(dialog)
}

pub struct SendBackDialog {
    dashboard: Entity<Dashboard>,
    task_id: String,
    /// The earlier stages as (id, name).
    options: Vec<(String, String)>,
    to: String,
    reason: Entity<Editor>,
    busy: bool,
    focus: FocusHandle,
    pick_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
}

impl SendBackDialog {
    /// `--shot send-back` types a reason.
    pub fn set_reason(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reason
            .update(cx, |editor, cx| editor.set_text(text, window, cx));
    }

    fn name_of(&self, id: &str) -> String {
        self.options
            .iter()
            .find(|(each, _)| each == id)
            .map_or_else(|| id.to_owned(), |(_, name)| name.clone())
    }

    fn open_picker(&mut self, event: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let options = self.options.clone();
        let picked = self.to.clone();
        let this = cx.weak_entity();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            options.into_iter().fold(menu, |menu, (id, name)| {
                let this = this.clone();
                let checked = id == picked;
                menu.toggleable_entry(name, checked, IconPosition::Start, None, move |_, cx| {
                    let id = id.clone();
                    this.update(cx, |this, cx| {
                        this.to = id;
                        cx.notify();
                    })
                    .ok();
                })
            })
        });
        MenuLayer::open(menu, event.position(), window, cx);
    }
}

impl EventEmitter<DismissEvent> for SendBackDialog {}

impl Focusable for SendBackDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for SendBackDialog {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        vec![
            self.pick_focus.clone(),
            self.reason.focus_handle(cx),
            self.cancel_focus.clone(),
            self.submit_focus.clone(),
        ]
    }

    fn busy(&self, _: &App) -> bool {
        self.busy
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let input = StageActionInput {
            action: StageAction::Reopen,
            stage: None,
            to: Some(self.to.clone()),
            reason: send_back_reason(&self.reason.read(cx).text(cx)),
        };
        let id = self.task_id.clone();
        let written = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.stage_action(&id, input, "Could not send the task back.", cx)
        });
        cx.spawn(async move |this, cx| {
            let sent = written.await.is_ok();
            this.update(cx, |this, cx| {
                this.busy = false;
                if sent {
                    cx.emit(DismissEvent);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

impl Render for SendBackDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picker = Button::new("send-back-to", self.name_of(&self.to))
            .style(ButtonStyle::Outlined)
            .full_width()
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .track_focus(&self.pick_focus)
            .disabled(self.options.is_empty())
            .on_click(cx.listener(|this, event, window, cx| this.open_picker(event, window, cx)));
        let frame = DialogFrame::new(
            "Send the task back",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .description("The stage runs again, and the agent gets your reason with its prompt.")
        .footer(cancel_button(
            "Cancel",
            &self.cancel_focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ))
        .footer(primary_button(
            "Send back",
            &self.submit_focus,
            false,
            self.busy,
            cx.listener(|this, _, window, cx| this.submit(window, cx)),
        ))
        .child(field("Back to", picker))
        .child(field("What should change?", input_box(&self.reason, cx)));
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
