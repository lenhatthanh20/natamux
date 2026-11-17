//! Text widgets: `Label` for a line of UI text and `Headline` for a title.

use std::borrow::Cow;

use gpui::{
    App, Div, FontWeight, IntoElement, ParentElement, Pixels, Rems, RenderOnce, SharedString,
    Styled, TextAlign, Window, div, px, relative, rems,
};
use theme::ActiveTheme;

use crate::color::Color;
use crate::control_chars;
use crate::spacing::rems_from_px;

/// How big the text of a label is.
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum LabelSize {
    /// 14 px.
    #[default]
    Default,
    /// 16 px.
    Large,
    /// 12 px.
    Small,
    /// 10 px.
    XSmall,
    Custom(Rems),
}

impl LabelSize {
    fn text_rems(self) -> Rems {
        let px: f32 = match self {
            Self::Custom(size) => return size,
            Self::XSmall => 10.,
            Self::Small => 12.,
            Self::Default => 14.,
            Self::Large => 16.,
        };
        rems_from_px(px)
    }
}

/// A named text size, for text drawn without a label.
#[derive(Debug, Default, Clone)]
pub enum TextSize {
    /// 14 px.
    #[default]
    Default,
    /// 16 px.
    Large,
    /// 12 px.
    Small,
    /// 10 px.
    XSmall,
    /// The UI font size of the settings.
    Ui,
    /// The buffer font size of the settings.
    Editor,
}

impl TextSize {
    /// In rems, so the size grows and shrinks with the UI size.
    pub fn rems(self, cx: &App) -> Rems {
        rems_from_px(self.pixels(cx))
    }

    /// In px. The four named sizes stay the same at every UI size; the state filter counts on it.
    pub fn pixels(self, cx: &App) -> Pixels {
        let fixed: f32 = match self {
            Self::XSmall => 10.,
            Self::Small => 12.,
            Self::Default => 14.,
            Self::Large => 16.,
            Self::Ui => return theme::theme_settings(cx).ui_font_size(cx),
            Self::Editor => return theme::theme_settings(cx).buffer_font_size(cx),
        };
        px(fixed)
    }
}

/// How tall one line of a label is.
#[derive(Default, PartialEq, Copy, Clone)]
pub enum LineHeightStyle {
    /// gpui's line height for the text: 1.618 times the text size.
    #[default]
    TextLabel,
    /// The line is exactly as tall as the text size.
    UiLabel,
}

/// The setters the app calls on a label.
pub trait LabelCommon {
    fn size(self, size: LabelSize) -> Self;
    fn weight(self, weight: FontWeight) -> Self;
    fn line_height_style(self, line_height_style: LineHeightStyle) -> Self;
    fn color(self, color: Color) -> Self;
    /// A line through the text.
    fn strikethrough(self) -> Self;
    /// Multiplies the alpha of the text colour.
    fn alpha(self, alpha: f32) -> Self;
    /// Keeps the text on one line and ends it with "…" where it does not fit.
    fn truncate(self) -> Self;
    /// Keeps the text on one line; a new line or another control character shows as a symbol.
    fn single_line(self) -> Self;
    /// Draws in the buffer font, at its weight. A weight set earlier is replaced; one set later
    /// wins (the key chip draws at 400 because of this).
    fn buffer_font(self, cx: &App) -> Self;
}

/// How a label treats text wider than its box. A later setter can only make it stricter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Flow {
    Wrap,
    OneLine,
    Ellipsis,
}

/// UI text: 14 px in the theme's text colour, unless a setter says otherwise.
#[derive(IntoElement)]
pub struct Label {
    text: SharedString,
    /// Holds what is set straight on the box: the buffer font, a line clamp, centring.
    frame: Div,
    size: LabelSize,
    tone: Color,
    fade: Option<f32>,
    weight: Option<FontWeight>,
    tight_lines: bool,
    struck: bool,
    flow: Flow,
}

impl Label {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Label {
            text: text.into(),
            frame: div(),
            size: LabelSize::default(),
            tone: Color::Default,
            fade: None,
            weight: None,
            tight_lines: false,
            struck: false,
            flow: Flow::Wrap,
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.text = text.into();
    }

    /// Wraps up to `lines` lines; the last one ends with "…" when the text goes on.
    pub fn line_clamp(self, lines: usize) -> Self {
        let frame = self.frame.line_clamp(lines).text_ellipsis();
        Label { frame, ..self }
    }

    pub fn text_center(mut self) -> Self {
        self.frame.style().text.text_align = Some(TextAlign::Center);
        self
    }

    fn with_flow(mut self, flow: Flow) -> Self {
        self.flow = self.flow.max(flow);
        self
    }
}

impl LabelCommon for Label {
    fn size(self, size: LabelSize) -> Self {
        Label { size, ..self }
    }

    fn weight(self, weight: FontWeight) -> Self {
        Label {
            weight: Some(weight),
            ..self
        }
    }

    fn line_height_style(self, line_height_style: LineHeightStyle) -> Self {
        Label {
            tight_lines: line_height_style == LineHeightStyle::UiLabel,
            ..self
        }
    }

    fn color(self, color: Color) -> Self {
        Label {
            tone: color,
            ..self
        }
    }

    fn strikethrough(self) -> Self {
        Label {
            struck: true,
            ..self
        }
    }

    fn alpha(self, alpha: f32) -> Self {
        Label {
            fade: Some(alpha),
            ..self
        }
    }

    fn truncate(self) -> Self {
        self.with_flow(Flow::Ellipsis)
    }

    fn single_line(mut self) -> Self {
        if let Cow::Owned(shown) = control_chars::visible(&self.text) {
            self.text = SharedString::from(shown);
        }
        self.with_flow(Flow::OneLine)
    }

    fn buffer_font(mut self, cx: &App) -> Self {
        let font = theme::theme_settings(cx).buffer_font(cx).clone();
        self.weight = Some(font.weight);
        self.frame = self.frame.font(font);
        self
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut tone = self.tone.color(cx);
        if let Some(alpha) = self.fade {
            tone.fade_out(1. - alpha);
        }
        let weight = self
            .weight
            .unwrap_or_else(|| theme::theme_settings(cx).ui_font(cx).weight);

        let mut el = self
            .frame
            .text_size(self.size.text_rems())
            .text_color(tone)
            .font_weight(weight);
        if self.tight_lines {
            el = el.line_height(relative(1.));
        }
        if self.struck {
            el = el.line_through();
        }
        if self.flow >= Flow::OneLine {
            el = el.whitespace_nowrap();
        }
        if self.flow == Flow::Ellipsis {
            el = el.min_w_0().overflow_x_hidden().text_ellipsis();
        }
        el.child(self.text)
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
        rems(match self {
            Self::XSmall => 0.88,
            Self::Small => 1.,
            Self::Medium => 1.125,
            Self::Large => 1.27,
            Self::XLarge => 1.43,
        })
    }

    /// The same at every size.
    pub fn line_height(self) -> Rems {
        rems(1.6)
    }
}

/// A title in the UI font. It is always drawn in the theme's text colour.
#[derive(IntoElement)]
pub struct Headline {
    text: SharedString,
    size: HeadlineSize,
}

impl Headline {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Headline {
            text: text.into(),
            size: HeadlineSize::Medium,
        }
    }

    pub fn size(self, size: HeadlineSize) -> Self {
        Headline { size, ..self }
    }

    /// There for call sites that colour every text widget; a headline ignores it.
    pub fn color(self, _: Color) -> Self {
        self
    }
}

impl RenderOnce for Headline {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ui_font = theme::theme_settings(cx).ui_font(cx).clone();
        let text = cx.theme().colors().text;
        div()
            .text_size(self.size.rems())
            .line_height(self.size.line_height())
            .font(ui_font)
            .text_color(text)
            .child(self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_and_single_line_give_an_ellipsis_in_either_order() {
        assert!(Label::new("a").truncate().single_line().flow == Flow::Ellipsis);
        assert!(Label::new("a").single_line().truncate().flow == Flow::Ellipsis);
        assert!(Label::new("a").single_line().flow == Flow::OneLine);
        assert!(Label::new("a").flow == Flow::Wrap);
    }

    #[test]
    fn single_line_shows_control_characters_as_symbols() {
        assert_eq!(Label::new("a\nb").single_line().text.as_ref(), "a\u{23ce}b");
    }
}
