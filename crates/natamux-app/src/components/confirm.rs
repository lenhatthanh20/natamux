//! The confirm dialog of a destructive action (`ConfirmDialog` in `ui/Dialog.tsx`): it says what
//! goes and what stays, Cancel has the focus, and the main button runs the action and closes. Both
//! buttons are grey (T-0168): the title and the label say what goes.

use std::rc::Rc;

use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable};
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
    /// The Settings sheet look (header 44, footer 48), for confirms opened from Settings.
    pub sheet: bool,
    /// A read the text depends on is still running: the main button and Mod+Enter wait for
    /// `ConfirmDialog::resolve`. Cancel, Esc and the scrim still close.
    pub checking: bool,
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
            sheet: false,
            checking: false,
        }
    }

    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    pub fn sheet(mut self) -> Self {
        self.sheet = true;
        self
    }

    pub fn checking(mut self) -> Self {
        self.checking = true;
        self
    }

    pub fn rows(mut self, rows: Vec<(SharedString, SharedString)>) -> Self {
        self.rows = rows;
        self
    }

    /// Shows the confirm with Cancel focused.
    pub fn open(self, window: &mut Window, cx: &mut App) {
        self.show(false, window, cx);
    }

    /// As `open`, and gives the dialog back, so a read that ends later can `resolve` it.
    pub fn open_view(self, window: &mut Window, cx: &mut App) -> Entity<ConfirmDialog> {
        self.show(false, window, cx)
    }

    /// Shows the confirm over the open dialog, which stays as it is under it.
    pub fn open_over(self, window: &mut Window, cx: &mut App) {
        self.show(true, window, cx);
    }

    fn show(self, over: bool, window: &mut Window, cx: &mut App) -> Entity<ConfirmDialog> {
        let view = cx.new(|cx| ConfirmDialog {
            confirm: self,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
            submit: cx.focus_handle(),
        });
        let cancel = view.read(cx).cancel.clone();
        if over {
            DialogLayer::open_over(view.clone(), window, cx);
        } else {
            DialogLayer::open(view.clone(), window, cx);
        }
        window.focus(&cancel, cx);
        view
    }
}

pub struct ConfirmDialog {
    pub(crate) confirm: Confirm,
    focus: FocusHandle,
    cancel: FocusHandle,
    submit: FocusHandle,
}

impl EventEmitter<DismissEvent> for ConfirmDialog {}

impl ConfirmDialog {
    /// The read is done: the text and action it decided, and the main button works.
    pub fn resolve(
        &mut self,
        description: impl Into<SharedString>,
        confirm_label: impl Into<SharedString>,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.confirm.description = description.into();
        self.confirm.confirm_label = confirm_label.into();
        self.confirm.on_confirm = Rc::new(on_confirm);
        self.confirm.checking = false;
        cx.notify();
    }
}

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
        if self.confirm.checking {
            return;
        }
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
                .max_h(rems_from_px(160.0_f32))
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
        let mut frame = DialogFrame::new(confirm.title.clone(), close);
        if confirm.sheet {
            // A sheet's header is 44 px high, so the text goes in the body (layout-b 6.3).
            frame = frame.sheet().body_padded().child(
                div()
                    .text_size(rems_from_px(12.0_f32))
                    .line_height(rems_from_px(18.0_f32))
                    .child(confirm.description.clone()),
            );
        } else {
            frame = frame.description(confirm.description.clone());
        }
        div().track_focus(&self.focus).child(
            frame
                .children(rows)
                .footer(cancel_button(
                    confirm.cancel_label.clone(),
                    &self.cancel,
                    cancel,
                ))
                .footer(primary_button(
                    confirm.confirm_label.clone(),
                    &self.submit,
                    confirm.checking,
                    submit,
                )),
        )
    }
}

#[cfg(test)]
#[path = "confirm_tests.rs"]
mod tests;
