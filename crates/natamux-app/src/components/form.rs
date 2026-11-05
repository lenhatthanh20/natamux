//! Form parts shared by dialogs and the edit form: a labelled field, a bordered text box, and a
//! text editor with a placeholder. `SettingsInput` is the Settings screen's smaller box.

use editor::Editor;
use gpui::Focusable;
use gpui::{Entity, FontWeight};
use language::language_settings::SoftWrap;
use ui::prelude::*;

use super::tab_order;

pub fn field(label: impl Into<SharedString>, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            Label::new(label)
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Muted),
        )
        .child(control)
}

pub fn input_box(editor: &Entity<Editor>, cx: &App) -> impl IntoElement {
    tab_order::push(&editor.focus_handle(cx));
    let colors = cx.theme().colors();
    div()
        .px_2p5()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(editor.clone())
}

/// The Settings screen's text box (layout-b 3.10): 24 px high, radius 5, padding 0 8, the
/// focus or error colour on its border. `area()` makes it a textarea: radius 6, padding 8 10.
#[derive(IntoElement)]
pub struct SettingsInput {
    editor: Entity<Editor>,
    width: Option<f32>,
    height: Option<f32>,
    radius: Option<f32>,
    area: bool,
    invalid: bool,
}

impl SettingsInput {
    pub fn new(editor: &Entity<Editor>) -> Self {
        Self {
            editor: editor.clone(),
            width: None,
            height: None,
            radius: None,
            area: false,
            invalid: false,
        }
    }

    /// Width in px; without it the box fills its field.
    pub fn width(mut self, px: f32) -> Self {
        self.width = Some(px);
        self
    }

    /// A textarea, for an editor made with `text_editor(.., Some(lines), ..)`.
    pub fn area(mut self) -> Self {
        self.area = true;
        self
    }

    /// A fixed height in px (the JSON tab's 456).
    pub fn height(mut self, px: f32) -> Self {
        self.height = Some(px);
        self
    }

    /// A corner radius in px other than 5 (6 for a textarea; the JSON tab uses 8).
    pub fn radius(mut self, px: f32) -> Self {
        self.radius = Some(px);
        self
    }

    /// The value is wrong: the border turns the error colour.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }
}

impl RenderOnce for SettingsInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = self.editor.focus_handle(cx);
        tab_order::push(&focus);
        let colors = cx.theme().colors();
        let border = if self.invalid {
            cx.theme().status().error
        } else if focus.contains_focused(window, cx) {
            colors.border_focused
        } else {
            colors.border
        };
        let radius = self.radius.unwrap_or(if self.area { 6. } else { 5. });
        let input = div()
            .min_w_0()
            .rounded(rems_from_px(radius))
            .border_1()
            .border_color(border)
            .bg(colors.editor_background);
        let input = if self.area {
            input.px(rems_from_px(10.0_f32)).py(rems_from_px(8.0_f32))
        } else {
            input
                .flex()
                .items_center()
                .h(rems_from_px(24.0_f32))
                .px(rems_from_px(8.0_f32))
        };
        input
            .map(|input| match self.width {
                Some(width) => input.flex_none().w(rems_from_px(width)),
                None => input.w_full(),
            })
            .when_some(self.height, |input, height| {
                input.h(rems_from_px(height)).overflow_hidden()
            })
            .child(self.editor)
    }
}

/// One line, or `lines` (min, max) of text that grows with what is typed.
pub fn text_editor(
    placeholder: &str,
    lines: Option<(usize, usize)>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Editor> {
    cx.new(|cx| {
        let mut editor = match lines {
            Some((min, max)) => {
                let mut editor = Editor::auto_height(min, max, window, cx);
                // A textarea wraps long lines; Zed's default setting does not.
                editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
                editor
            }
            None => Editor::single_line(window, cx),
        };
        editor.set_placeholder_text(placeholder, window, cx);
        editor
    })
}
