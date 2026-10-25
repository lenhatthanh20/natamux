//! The state shape of a task or terminal, tinted with its state colour (docs/ui-spec.md 4.10). A
//! running shape blinks once every 2 s (`motion_rules::beat`); nothing else moves.

use gpui::{Transformation, size, svg};
use natamux_proto::tasks::task::TaskState;
use ui::{Tooltip, prelude::*};

use super::colors::{signal, state_colors};
use super::motion;
use super::motion_rules::{Beat, Moving, beat};
use super::shape::{ShapeKind, ShapeSize};

/// The shape of `state`, `None` while the task is not loaded. A quiet agent shows the half ring in
/// the signal colour. With `tip`, the tooltip is the label and the tip, as the web's `data-tip`.
#[derive(IntoElement)]
pub struct StateGlyph {
    id: ElementId,
    state: Option<TaskState>,
    quiet: bool,
    size: ShapeSize,
    tip: Option<SharedString>,
}

impl StateGlyph {
    pub fn new(id: impl Into<ElementId>, state: Option<TaskState>) -> Self {
        Self {
            id: id.into(),
            state,
            quiet: false,
            size: ShapeSize::Task,
            tip: None,
        }
    }

    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    pub fn size(mut self, size: ShapeSize) -> Self {
        self.size = size;
        self
    }

    pub fn tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tip = Some(tip.into());
        self
    }
}

impl RenderOnce for StateGlyph {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let kind = ShapeKind::of(self.state, self.quiet);
        let Beat { opacity, scale } = if kind == ShapeKind::Running {
            beat(motion::frame(Moving::Beat, window, cx))
        } else {
            beat(None)
        };
        let color = match (self.state, kind) {
            (_, ShapeKind::Quiet) => signal(cx).fg,
            (Some(state), _) => state_colors(state, cx).fg,
            (None, _) => cx.theme().colors().text_muted,
        };
        let label = match &self.tip {
            Some(tip) => format!("{} · {tip}", kind.label()),
            None => kind.label().to_string(),
        };
        let edge = self.size.box_px();
        div()
            .id(self.id)
            .flex_none()
            .size(px(edge))
            .child(
                svg()
                    .path(kind.svg_path(self.size))
                    .size(px(edge))
                    .flex_none()
                    .with_transformation(Transformation::scale(size(scale, scale)))
                    .text_color(color.opacity(opacity)),
            )
            .tooltip(Tooltip::text(label))
    }
}
