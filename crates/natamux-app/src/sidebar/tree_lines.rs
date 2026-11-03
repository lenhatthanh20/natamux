//! Which lines of the sidebar list changed, so the list measures only those again.

use crate::sidebar::tree_model::Line;

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
