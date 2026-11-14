//! The fewest edits that turn one text into another (plan 6.3.2, `replace_minimal`): lines that
//! differ come from `imara-diff`; inside a changed run of lines only the chars between the common
//! start and end are replaced.

use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput};

/// `range` of the old text becomes `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// The byte offset where each line starts, plus the end of the text.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.match_indices('\n')
            .map(|(at, _)| at + 1)
            .filter(|&at| at < text.len()),
    );
    starts.push(text.len());
    starts
}

/// The edits from `old` to `new`, front to back, with ranges of `old`.
pub fn edits(old: &str, new: &str) -> Vec<Edit> {
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let old_lines = line_starts(old);
    let new_lines = line_starts(new);
    let byte = |starts: &[usize], line: u32| starts[(line as usize).min(starts.len() - 1)];
    diff.hunks()
        .filter_map(|hunk| {
            let old_range = byte(&old_lines, hunk.before.start)..byte(&old_lines, hunk.before.end);
            let new_range = byte(&new_lines, hunk.after.start)..byte(&new_lines, hunk.after.end);
            trim(old, old_range, &new[new_range])
        })
        .collect()
}

/// `old[range]` becomes `new`, less the chars both start and end with.
fn trim(old: &str, range: Range<usize>, new: &str) -> Option<Edit> {
    let before = &old[range.clone()];
    let start = before
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(before.len().min(new.len()), |((at, _), _)| at);
    let start = super::motion::floor_boundary(before, start.min(new.len()));
    let start = super::motion::floor_boundary(new, start);
    let (before_rest, new_rest) = (&before[start..], &new[start..]);
    let end = before_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    let removed = start..before.len() - end;
    let inserted = &new[start..new.len() - end];
    if removed.is_empty() && inserted.is_empty() {
        return None;
    }
    Some(Edit {
        range: range.start + removed.start..range.start + removed.end,
        text: inserted.to_string(),
    })
}

/// Where `at` of the old text is after `edits`: before an edit it stays, after one it moves by
/// the change of length, inside one it keeps its distance from the start up to the new text's end.
pub fn map_offset(edits: &[Edit], at: usize) -> usize {
    let mut shift: isize = 0;
    for edit in edits {
        if at < edit.range.start {
            break;
        }
        if at < edit.range.end {
            let inside = (at - edit.range.start).min(edit.text.len());
            return (edit.range.start as isize + shift) as usize + inside;
        }
        shift += edit.text.len() as isize - edit.range.len() as isize;
    }
    (at as isize + shift) as usize
}

#[cfg(test)]
#[path = "minimal_tests.rs"]
mod tests;
