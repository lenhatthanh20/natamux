//! A small bordered tag of text, such as a version.

use gpui::{
    App, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce, SharedString,
    Styled, Window, prelude::FluentBuilder,
};
use theme::ActiveTheme;

use crate::color::Color;
use crate::label::{Label, LabelCommon, LabelSize};
use crate::spacing::h_flex;

/// Text in the buffer font, extra small by default, in a box with a 1 px border.
#[derive(IntoElement)]
pub struct Chip {
    label: SharedString,
    label_color: Color,
    label_size: LabelSize,
    background: Option<Hsla>,
    border: Option<Hsla>,
    height: Option<Pixels>,
}

impl Chip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            label_color: Color::Default,
            label_size: LabelSize::XSmall,
            background: None,
            border: None,
            height: None,
        }
    }

    pub fn label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    pub fn bg_color(mut self, color: Hsla) -> Self {
        self.background = Some(color);
        self
    }

    pub fn border_color(mut self, color: Hsla) -> Self {
        self.border = Some(color);
        self
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let background = self.background.unwrap_or(colors.element_background);
        let border = self.border.unwrap_or(colors.border);
        let label = Label::new(self.label.clone())
            .size(self.label_size)
            .color(self.label_color)
            .buffer_font(cx)
            .truncate();
        h_flex()
            .when_some(self.height, |chip, height| chip.h(height))
            .flex_none()
            .gap_0p5()
            .px_1()
            .border_1()
            .rounded_sm()
            .border_color(border)
            .bg(background)
            .overflow_hidden()
            .child(label)
            .id(self.label)
    }
}
