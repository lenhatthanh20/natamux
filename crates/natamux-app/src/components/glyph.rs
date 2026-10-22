//! The state shape of a task or terminal, tinted with its state colour (docs/ui-spec.md 4.10).

use gpui::svg;
use natamux_proto::tasks::task::TaskState;
use ui::{Tooltip, prelude::*};

use super::colors::{signal, state_colors};
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let kind = ShapeKind::of(self.state, self.quiet);
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
                    .text_color(color),
            )
            .tooltip(Tooltip::text(label))
    }
}
