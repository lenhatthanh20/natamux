//! The confirm dialog of a destructive action (`ConfirmDialog` in `ui/Dialog.tsx`): it says what
//! goes and what stays, Cancel has the focus, and the danger button runs the action and closes.

use std::rc::Rc;

use gpui::{DismissEvent, EventEmitter, FocusHandle, Focusable};
use ui::prelude::*;

use super::dialog::{DialogFrame, DialogLayer, DialogView, cancel_button, primary_button};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a confirm says and does.
pub struct Confirm {
    pub title: SharedString,
    pub description: SharedString,
    pub confirm_label: SharedString,
    pub cancel_label: SharedString,
    /// Rows under the description, as (id, title): the tasks a confirm acts on.
    pub rows: Vec<(SharedString, SharedString)>,
    pub on_confirm: Run,
}

impl Confirm {
    pub fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        confirm_label: impl Into<SharedString>,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Confirm {
            title: title.into(),
            description: description.into(),
            confirm_label: confirm_label.into(),
            cancel_label: "Cancel".into(),
            rows: Vec::new(),
            on_confirm: Rc::new(on_confirm),
        }
    }

    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    pub fn rows(mut self, rows: Vec<(SharedString, SharedString)>) -> Self {
        self.rows = rows;
        self
    }

    /// Shows the confirm with Cancel focused.
    pub fn open(self, window: &mut Window, cx: &mut App) {
        let view = cx.new(|cx| ConfirmDialog {
            confirm: self,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
            submit: cx.focus_handle(),
        });
        let cancel = view.read(cx).cancel.clone();
        DialogLayer::open(view, window, cx);
        window.focus(&cancel, cx);
    }
}

pub struct ConfirmDialog {
    confirm: Confirm,
    focus: FocusHandle,
    cancel: FocusHandle,
    submit: FocusHandle,
}

impl EventEmitter<DismissEvent> for ConfirmDialog {}

impl Focusable for ConfirmDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for ConfirmDialog {
    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        vec![self.cancel.clone(), self.submit.clone()]
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Close first, so a dialog the action opens is not closed with this one.
        let run = self.confirm.on_confirm.clone();
        cx.emit(DismissEvent);
        run(window, cx);
    }
}

impl Render for ConfirmDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let confirm = &self.confirm;
        let rows = (!confirm.rows.is_empty()).then(|| {
            v_flex()
                .id("confirm-rows")
                .max_h(px(160.))
                .overflow_y_scroll()
                .gap_0p5()
                .children(confirm.rows.iter().map(|(id, title)| {
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .child(
                            Label::new(id.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .buffer_font(cx),
                        )
                        .child(Label::new(title.clone()).size(LabelSize::Small).truncate())
                }))
        });
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let submit = cx.listener(|this, _, window, cx| this.submit(window, cx));
        div().track_focus(&self.focus).child(
            DialogFrame::new(confirm.title.clone(), close)
                .description(confirm.description.clone())
                .children(rows)
                .footer(cancel_button(
                    confirm.cancel_label.clone(),
                    &self.cancel,
                    cancel,
                ))
                .footer(primary_button(
                    confirm.confirm_label.clone(),
                    &self.submit,
                    true,
                    false,
                    submit,
                )),
        )
    }
}
