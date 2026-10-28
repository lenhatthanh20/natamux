//! The grid of terminal panes that the workspace and the Agents grid share (docs/ui-spec.md
//! 3.1 Panes): rows whose last pane fills its row, and the skeleton while it waits for a list.

use ui::prelude::*;

use super::pane_rules::pane_span;
use crate::components::skeleton::SkeletonList;

/// The pane gap and padding of 3.1.
pub const GAP: f32 = 8.;
/// `minmax(12rem, 1fr)`: a row is at least this high.
const MIN_ROW: f32 = 192.;

/// The grid while it waits for the list of the task's terminals.
pub fn skeleton() -> AnyElement {
    div()
        .size_full()
        .p(px(GAP))
        .child(SkeletonList::new(6, 28.))
        .into_any_element()
}

/// Puts panes in rows of `columns`; the last pane of the last row takes the rest of it
/// (`paneSpan`, as grid spans in flex rows).
pub fn grid_rows(panes: Vec<AnyElement>, columns: usize) -> Vec<AnyElement> {
    let count = panes.len();
    let columns = columns.max(1);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for (index, pane) in panes.into_iter().enumerate() {
        let mut cell = div()
            .min_w_0()
            .h_full()
            .flex_basis(relative(0.))
            .child(pane);
        cell.style().flex_grow = Some(pane_span(index, count, columns) as f32);
        row.push(cell);
        if (index + 1) % columns == 0 || index + 1 == count {
            rows.push(
                h_flex()
                    .flex_1()
                    .min_h(px(MIN_ROW))
                    .gap(px(GAP))
                    .children(std::mem::take(&mut row))
                    .into_any_element(),
            );
        }
    }
    rows
}
