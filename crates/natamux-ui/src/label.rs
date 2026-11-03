//! Text: a label in the UI font (or the buffer font), and a headline.

use gpui::{
    App, Div, FontWeight, IntoElement, ParentElement, Rems, RenderOnce, SharedString, Styled,
    TextAlign, UnderlineStyle, Window, div, prelude::FluentBuilder, px, relative, rems,
};
use theme::ActiveTheme;
use zed_ui::{Color, LabelSize};

use crate::control_chars;
use crate::spacing::rems_from_px;

/// The line height of a label.
#[derive(Default, PartialEq, Copy, Clone)]
pub enum LineHeightStyle {
    /// The text's own line height (1.618 times its size, from gpui).
    #[default]
    TextLabel,
    /// One line is exactly as high as the text size.
    UiLabel,
}

/// The setters every label takes.
pub trait LabelCommon {
    fn size(self, size: LabelSize) -> Self;
    fn weight(self, weight: FontWeight) -> Self;
    fn line_height_style(self, line_height_style: LineHeightStyle) -> Self;
    fn color(self, color: Color) -> Self;
    fn strikethrough(self) -> Self;
    fn italic(self) -> Self;
    fn underline(self) -> Self;
    /// The colour's alpha times this.
    fn alpha(self, alpha: f32) -> Self;
    /// One line, cut with an ellipsis at the end when it does not fit.
    fn truncate(self) -> Self;
    /// One line; control characters show as symbols.
    fn single_line(self) -> Self;
    /// The buffer font. It also takes the buffer font's weight, so a `weight` set before it is
    /// lost (the key chip relies on this).
    fn buffer_font(self, cx: &App) -> Self;
}

/// A line of UI text. Size 14 and the default text colour unless set.
#[derive(IntoElement)]
pub struct Label {
    base: Div,
    text: SharedString,
    size: LabelSize,
    weight: Option<FontWeight>,
    line_height_style: LineHeightStyle,
    color: Color,
    strikethrough: bool,
    italic: bool,
    underline: bool,
    alpha: Option<f32>,
    single_line: bool,
    truncate: bool,
}

impl Label {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            base: div(),
            text: text.into(),
            size: LabelSize::Default,
            weight: None,
            line_height_style: LineHeightStyle::TextLabel,
            color: Color::Default,
            strikethrough: false,
            italic: false,
            underline: false,
            alpha: None,
            single_line: false,
            truncate: false,
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.text = text.into();
    }

    /// Wraps, and cuts with an ellipsis at the end of the last of `lines` lines.
    pub fn line_clamp(mut self, lines: usize) -> Self {
        self.base = self.base.line_clamp(lines).text_ellipsis();
        self
    }

    pub fn text_center(mut self) -> Self {
        self.base.style().text.text_align = Some(TextAlign::Center);
        self
    }
}

impl LabelCommon for Label {
    fn size(mut self, size: LabelSize) -> Self {
        self.size = size;
        self
    }

    fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    fn line_height_style(mut self, line_height_style: LineHeightStyle) -> Self {
        self.line_height_style = line_height_style;
        self
    }

    fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = Some(alpha);
        self
    }

    fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }

    fn single_line(mut self) -> Self {
        if let std::borrow::Cow::Owned(shown) = control_chars::visible(&self.text) {
            self.text = shown.into();
        }
        self.single_line = true;
        self
    }

    fn buffer_font(mut self, cx: &App) -> Self {
        let font = theme::theme_settings(cx).buffer_font(cx).clone();
        self.weight = Some(font.weight);
        self.base = self.base.font(font);
        self
    }
}

/// The text size of a label size.
fn text_size(size: LabelSize) -> Rems {
    match size {
        LabelSize::Large => rems_from_px(16_f32),
        LabelSize::Default => rems_from_px(14_f32),
        LabelSize::Small => rems_from_px(12_f32),
        LabelSize::XSmall => rems_from_px(10_f32),
        LabelSize::Custom(size) => size,
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut color = self.color.color(cx);
        if let Some(alpha) = self.alpha {
            color.fade_out(1. - alpha);
        }
        let weight = self
            .weight
            .unwrap_or(theme::theme_settings(cx).ui_font(cx).weight);
        let underline = UnderlineStyle {
            thickness: px(1.),
            color: Some(cx.theme().colors().text_muted.opacity(0.4)),
            wavy: false,
        };
        self.base
            .text_size(text_size(self.size))
            .when(self.line_height_style == LineHeightStyle::UiLabel, |this| {
                this.line_height(relative(1.))
            })
            .when(self.italic, |this| this.italic())
            .when(self.underline, |mut this| {
                this.text_style().underline = Some(underline);
                this
            })
            .when(self.strikethrough, |this| this.line_through())
            .when(self.single_line, |this| this.whitespace_nowrap())
            .when(self.truncate, |this| {
                this.min_w_0()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
            })
            .text_color(color)
            .font_weight(weight)
            .child(self.text)
    }
}

/// The size of a headline.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum HeadlineSize {
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

impl HeadlineSize {
    pub fn rems(self) -> Rems {
        match self {
            HeadlineSize::XSmall => rems(0.88),
            HeadlineSize::Small => rems(1.),
            HeadlineSize::Medium => rems(1.125),
            HeadlineSize::Large => rems(1.27),
            HeadlineSize::XLarge => rems(1.43),
        }
    }

    pub fn line_height(self) -> Rems {
        rems(1.6)
    }
}

/// A title in the UI font, always in the theme's text colour.
#[derive(IntoElement)]
pub struct Headline {
    size: HeadlineSize,
    text: SharedString,
}

impl Headline {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            size: HeadlineSize::default(),
            text: text.into(),
        }
    }

    pub fn size(mut self, size: HeadlineSize) -> Self {
        self.size = size;
        self
    }

    /// Takes a colour like the other text widgets, but a headline keeps the text colour.
    pub fn color(self, _color: Color) -> Self {
        self
    }
}

impl RenderOnce for Headline {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let font = theme::theme_settings(cx).ui_font(cx).clone();
        div()
            .font(font)
            .line_height(self.size.line_height())
            .text_size(self.size.rems())
            .text_color(cx.theme().colors().text)
            .child(self.text)
    }
}
