//! The marker of a step (`StageMarker.tsx`), used by the Progress board: a done
//! stage has the check, the current one the shape of its state, a skipped one the dash, and one
//! not reached yet an empty dashed ring. A branch is a diamond. A running stage's glyph blinks
//! (`StateGlyph`); nothing else moves.

use gpui::svg;
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::tasks::task::TaskState;
use ui::{Tooltip, prelude::*};

use super::stage_view_rules::marker_label;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::icons::icon_path;

/// `size` 18 of `StepRow`; a branch's diamond is 16.
const NODE: f32 = 18.;
const DIAMOND: f32 = 16.;

/// The state whose shape and colour the marker takes.
fn shown_state(status: StageStatus, state: Option<TaskState>) -> Option<TaskState> {
    match status {
        StageStatus::Done => Some(TaskState::Done),
        StageStatus::Current => Some(state.unwrap_or(TaskState::Ready)),
        StageStatus::Skipped => Some(TaskState::Cancelled),
        StageStatus::Upcoming | StageStatus::NotTaken => None,
    }
}

/// `key`: the step id, so each marker's tooltip has an id of its own.
pub fn marker(
    key: &str,
    status: StageStatus,
    state: Option<TaskState>,
    branch: bool,
    cx: &App,
) -> AnyElement {
    let label = marker_label(status, state);
    let shown = shown_state(status, state);
    if branch {
        let color = shown.map_or(cx.theme().colors().text_muted, |state| {
            state_colors(state, cx).fg
        });
        return div()
            .id(SharedString::from(format!("diamond-{key}")))
            .size(px(NODE))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path(icon_path("diamond"))
                    .size(px(DIAMOND))
                    .text_color(color),
            )
            .tooltip(Tooltip::text(label))
            .into_any_element();
    }
    match shown {
        Some(state) => div()
            .size(px(NODE))
            .flex()
            .items_center()
            .justify_center()
            .child(
                StateGlyph::new(SharedString::from(format!("marker-{key}")), Some(state))
                    .tip(label),
            )
            .into_any_element(),
        None => div()
            .id(SharedString::from(format!("ring-{key}")))
            .size(px(NODE))
            .rounded_full()
            .border_1()
            .border_dashed()
            .border_color(cx.theme().colors().border)
            .tooltip(Tooltip::text(label))
            .into_any_element(),
    }
}
