//! Form parts shared by dialogs and the edit form: a labelled field, a bordered text box, and a
//! text editor with a placeholder. `SettingsInput` is the Settings screen's smaller box.
//! Every input editor comes from `input_editor`, so all inputs share one text style.

use editor::{CurrentLineHighlight, Editor};
use gpui::Focusable;
use gpui::{Entity, FontWeight, TextStyleRefinement, relative};
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
    bar_edge: bool,
    fill: bool,
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
            bar_edge: false,
            fill: false,
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

    /// A textarea with only left padding, so the editor's own scrollbar runs along the right
    /// border from top to bottom.
    pub fn bar_edge(mut self) -> Self {
        self.area = true;
        self.bar_edge = true;
        self
    }

    /// A textarea that fills the height of its column (the stage sheet's prompt, T-0313), for an
    /// editor made with `FILL_LINES`: radius 8, padding 10 12, the editor scrolls inside.
    pub fn fill(mut self) -> Self {
        self.area = true;
        self.fill = true;
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
        let radius = self.radius.unwrap_or(if self.fill {
            8.
        } else if self.area {
            6.
        } else {
            5.
        });
        let input = div()
            .min_w_0()
            .rounded(rems_from_px(radius))
            .border_1()
            .border_color(border)
            .bg(colors.editor_background);
        let input = if self.fill {
            input
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .px(rems_from_px(12.0_f32))
                .py(rems_from_px(10.0_f32))
        } else if self.area {
            input
                .pl(rems_from_px(10.0_f32))
                .when(!self.bar_edge, |input| {
                    input.pr(rems_from_px(10.0_f32)).py(rems_from_px(8.0_f32))
                })
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

/// Inputs draw the cursor one line high. Zed's default buffer line (1.618) makes it fill a
/// 24 px box, so inputs use Zed's "standard" line instead.
pub const INPUT_LINE_HEIGHT: f32 = 1.3;

/// The text style of every input: the input line height, and a font family or size in px for
/// the fields that need one (mono ids, commands, branches).
pub fn input_style(family: Option<SharedString>, size: Option<f32>) -> TextStyleRefinement {
    TextStyleRefinement {
        font_family: family,
        font_size: size.map(|px| rems_from_px(px).into()),
        line_height: Some(relative(INPUT_LINE_HEIGHT)),
        ..Default::default()
    }
}

/// `lines` of an editor that fills its box and scrolls inside (`SettingsInput::fill`).
pub const FILL_LINES: (usize, usize) = (0, 0);

/// An input editor with the input text style: one line, `lines` (min, max) that grow, or
/// `FILL_LINES`.
pub fn input_editor(
    lines: Option<(usize, usize)>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Editor {
    let mut editor = match lines {
        Some(FILL_LINES) => {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_show_gutter(false, cx);
            editor.set_show_line_numbers(false, cx);
            editor.set_show_wrap_guides(false, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_current_line_highlight(Some(CurrentLineHighlight::None));
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor
        }
        Some((min, max)) => {
            let mut editor = Editor::auto_height(min, max, window, cx);
            // A textarea wraps long lines; Zed's default setting does not.
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor
        }
        None => Editor::single_line(window, cx),
    };
    editor.set_text_style_refinement(input_style(None, None));
    editor
}

/// One line, or `lines` (min, max) of text that grows with what is typed.
pub fn text_editor(
    placeholder: &str,
    lines: Option<(usize, usize)>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Editor> {
    cx.new(|cx| {
        let mut editor = input_editor(lines, window, cx);
        editor.set_placeholder_text(placeholder, window, cx);
        editor
    })
}

#[cfg(test)]
#[path = "form_style_tests.rs"]
mod tests;
