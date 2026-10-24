//! The Terminal font dialog (docs/ui-spec.md 4.34, `TerminalFontDialog.tsx`): a family and a size
//! for the terminals, with a preview. Empty fields use the built-in font.

use editor::{Editor, EditorEvent};
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Subscription};
use ui::{Tooltip, prelude::*};

use super::terminal_font::{
    BUILT_IN_FAMILY, BUILT_IN_SIZE, PREVIEW, RESET_LABEL, SIZE_REASON, clean_family,
    dialog_description, font_to_save, parse_terminal_font, size_ok,
};
use crate::app::dashboard::Dashboard;
use crate::components::button_style::quiet_button;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogSize, DialogView, cancel_button, primary_button,
};
use crate::components::form::{field, text_editor};
use crate::components::tab_order;

/// Opens the dialog with the family field focused. Over the Settings dialog it takes its place.
pub fn open(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let dashboard = dashboard.clone();
    let view = cx.new(|cx| TerminalFontDialog::new(dashboard, window, cx));
    let family = view.read(cx).family.clone();
    DialogLayer::open(view, window, cx);
    window.focus(&family.focus_handle(cx), cx);
}

pub struct TerminalFontDialog {
    dashboard: Entity<Dashboard>,
    family: Entity<Editor>,
    size: Entity<Editor>,
    /// Whether a custom font is saved: only then the reset button shows.
    has_custom: bool,
    focus: FocusHandle,
    reset: FocusHandle,
    cancel: FocusHandle,
    submit: FocusHandle,
    _typing: Vec<Subscription>,
}

impl TerminalFontDialog {
    fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let custom = dashboard
            .read(cx)
            .view()
            .terminal_font
            .as_ref()
            .and_then(parse_terminal_font);
        let family = text_editor(BUILT_IN_FAMILY, None, window, cx);
        let size = text_editor(&BUILT_IN_SIZE.to_string(), None, window, cx);
        if let Some(font) = &custom {
            if let Some(name) = &font.family {
                family.update(cx, |editor, cx| editor.set_text(name.clone(), window, cx));
            }
            if let Some(points) = font.size {
                size.update(cx, |editor, cx| {
                    editor.set_text(points.to_string(), window, cx)
                });
            }
        }
        let typing = [&family, &size]
            .into_iter()
            .map(|editor| {
                cx.subscribe(editor, |_, _, event: &EditorEvent, cx| {
                    if matches!(event, EditorEvent::BufferEdited) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            dashboard,
            family,
            size,
            has_custom: custom.is_some(),
            focus: cx.focus_handle(),
            reset: cx.focus_handle(),
            cancel: cx.focus_handle(),
            submit: cx.focus_handle(),
            _typing: typing,
        }
    }

    fn size_valid(&self, cx: &App) -> bool {
        let text = self.size.read(cx).text(cx);
        size_ok(&text, BUILT_IN_SIZE).is_some()
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.size_valid(cx) {
            return;
        }
        let font = font_to_save(&self.family.read(cx).text(cx), &self.size.read(cx).text(cx));
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.set_terminal_font(font.map(|font| font.to_json()), cx)
        });
        cx.emit(DismissEvent);
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_terminal_font(None, cx));
        cx.emit(DismissEvent);
    }

    /// The text of the preview box in the typed font, or in the fallback for a blank part.
    fn preview(&self, cx: &App) -> impl IntoElement + use<> {
        let typed = clean_family(&self.family.read(cx).text(cx));
        let family = if typed.is_empty() {
            BUILT_IN_FAMILY.to_owned()
        } else {
            typed
        };
        let size = size_ok(&self.size.read(cx).text(cx), BUILT_IN_SIZE).unwrap_or(BUILT_IN_SIZE);
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().editor_background)
            .font_family(family)
            .text_size(px(size))
            .child(PREVIEW)
    }
}

fn input(editor: &Entity<Editor>, invalid: bool, cx: &App) -> Div {
    tab_order::push(&editor.focus_handle(cx));
    let border = if invalid {
        cx.theme().status().error_border
    } else {
        cx.theme().colors().border
    };
    div()
        .px_2p5()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(cx.theme().colors().editor_background)
        .child(editor.clone())
}

impl EventEmitter<DismissEvent> for TerminalFontDialog {}

impl Focusable for TerminalFontDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for TerminalFontDialog {
    fn size(&self) -> DialogSize {
        DialogSize::Small
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = vec![self.family.focus_handle(cx), self.size.focus_handle(cx)];
        if self.has_custom {
            ring.push(self.reset.clone());
        }
        ring.extend([self.cancel.clone(), self.submit.clone()]);
        ring
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.save(cx);
    }
}

impl Render for TerminalFontDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let invalid = !self.size_valid(cx);
        let size_row = div()
            .w(rems_from_px(96.0_f32))
            .child(input(&self.size, invalid, cx));
        let save = primary_button(
            "Save",
            &self.submit,
            invalid,
            cx.listener(|this, _, _, cx| this.save(cx)),
        )
        .tooltip(Tooltip::text(if invalid {
            SIZE_REASON.to_owned()
        } else {
            dialog_description()
        }));
        let reset = self.has_custom.then(|| {
            quiet_button("terminal-font-reset", RESET_LABEL)
                .track_focus(&self.reset)
                .on_click(cx.listener(|this, _, _, cx| this.reset(cx)))
        });
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let mut frame = DialogFrame::new("Terminal font", close)
            .child(field("Font family", input(&self.family, false, cx)))
            .child(field("Size (px)", size_row))
            .child(self.preview(cx));
        if let Some(reset) = reset {
            frame = frame.footer(reset).footer(div().flex_1());
        }
        div().track_focus(&self.focus).child(
            frame
                .footer(cancel_button("Cancel", &self.cancel, cancel))
                .footer(save),
        )
    }
}
