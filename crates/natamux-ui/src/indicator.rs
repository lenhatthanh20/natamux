//! A small coloured dot that marks a state.

use gpui::{App, IntoElement, RenderOnce, Styled, Window, div};

use crate::color::Color;

/// A 6 px round dot.
#[derive(IntoElement)]
pub struct Indicator {
    color: Color,
}

impl Indicator {
    pub fn dot() -> Self {
        Self {
            color: Color::Default,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .w_1p5()
            .h_1p5()
            .rounded_full()
            .bg(self.color.color(cx))
    }
}
