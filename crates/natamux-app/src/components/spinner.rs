//! The busy spinner (web `ui/Spinner.tsx`): 8 dashes, a bright head and a fading tail, one dash per
//! 125 ms tick. Still (a window without focus, `--shot`), it shows three dots.

use std::f32::consts::FRAC_PI_4;

use gpui::{Transformation, radians, svg};
use ui::prelude::*;

use super::motion;
use super::motion_rules::{Moving, spin_step};

const DASHES: &str = "icons/motion/spin-dashes.svg";
const DOTS: &str = "icons/motion/spin-dots.svg";

/// In place of a still icon of the same size, muted as the icons it replaces.
#[derive(IntoElement)]
pub struct Spinner {
    size: IconSize,
}

impl Spinner {
    pub fn new(size: IconSize) -> Self {
        Self { size }
    }
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let art = svg()
            .size(self.size.rems())
            .flex_none()
            .text_color(Color::Muted.color(cx));
        match spin_step(motion::frame(Moving::Spin, window, cx)) {
            Some(step) => art
                .path(DASHES)
                .with_transformation(Transformation::rotate(radians(f32::from(step) * FRAC_PI_4))),
            None => art.path(DOTS),
        }
    }
}
