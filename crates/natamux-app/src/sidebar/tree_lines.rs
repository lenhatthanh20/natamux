//! Which lines of the sidebar list changed, so the list measures only those again, and what a
//! change of the dashboard asks of the tree.

use crate::app::dashboard_data::{Changed, Part};
use crate::sidebar::tree_model::Line;

/// What the tree builds from: the lines and the terminals each row reads.
const MODEL_PARTS: &[Part] = &[Part::Tasks, Part::Repos, Part::Terminals, Part::View];

/// What a change of the dashboard asks of the tree.
pub(super) struct TreeNews {
    /// The lines are built again.
    pub(super) build: bool,
    /// The rows read the dashboard again and the tree draws.
    pub(super) draw: bool,
    /// Every row is measured again.
    pub(super) measure: bool,
}

/// The tree shows neither the selection nor a panel width, so a key move in the table and a
/// drag of a panel edge ask nothing of it. The list measures its rows again by itself when its
/// width changes.
pub(super) fn tree_news(changed: &Changed) -> TreeNews {
    let build = changed.any_exact(MODEL_PARTS);
    TreeNews {
        build,
        // Cards also show the activity, the waiting boxes and the diff stat.
        draw: build || changed.any_exact(&[Part::Activity, Part::Events, Part::DiffStats]),
        // The waiting box and the blur change a card's height, not the lines.
        measure: changed.any_exact(&[Part::Events, Part::View]),
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum LinesChange {
    Same,
    /// The same count, with these lines changed.
    Range(std::ops::Range<usize>),
    Count,
}

/// Which lines of the list to measure again.
pub(super) fn changed_lines(old: &[Line], new: &[Line]) -> LinesChange {
    if old.len() != new.len() {
        return LinesChange::Count;
    }
    let differs = |(a, b): (&Line, &Line)| a != b;
    let Some(first) = old.iter().zip(new).position(differs) else {
        return LinesChange::Same;
    };
    let last = old.iter().zip(new).rposition(differs).unwrap_or(first);
    LinesChange::Range(first..last + 1)
}
