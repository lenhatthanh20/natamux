//! The diff of the Changes tab as plain data (plan 6.5): the hunks of a line diff, the changed
//! words of small hunks, and the rows each side of the view shows, in side by side or inline,
//! with the whole file or 3 lines around each change. T-DIFF-1 holds what the old view gave.
//!
//! Both texts have their line endings made `\n` first, as an editor buffer keeps them, so rows
//! and byte offsets are those of the shown text.

// The view moves onto this model in the next part of S6.2; until then only `line_diff` is used.
#![cfg_attr(not(test), allow(dead_code))]

use std::ops::{Range, RangeInclusive};

use imara_diff::{Algorithm, Diff, InternedInput};

use super::change_rules::Side;
use super::diff_rules::{HunkRows, end_spacer, spacers};
use super::languages::lang_for;

/// Lines kept around each change while unchanged lines are collapsed (Monaco's context).
pub const CONTEXT_LINES: u32 = 3;
/// Hunks with more lines than this on a side get no word diff.
const MAX_WORD_DIFF_LINES: u32 = 5;

/// One hunk: its rows, and the changed words when both sides have the same number of lines (at
/// most 5). Old words are byte ranges of the hunk's old text, new words of the whole new text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub rows: HunkRows,
    pub old_words: Vec<Range<usize>>,
    pub new_words: Vec<Range<usize>>,
}

/// The two texts and their hunks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDiff {
    pub old: String,
    pub new: String,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    /// All rows of the old side, the empty one after a final line break included.
    pub fn old_rows(&self) -> u32 {
        rows(&self.old)
    }

    pub fn new_rows(&self) -> u32 {
        rows(&self.new)
    }

    /// The byte where the hunk's old text starts.
    pub fn old_start(&self, hunk: &Hunk) -> usize {
        line_start(&self.old, hunk.rows.old.start)
    }

    pub fn hunk_rows(&self) -> Vec<HunkRows> {
        self.hunks.iter().map(|hunk| hunk.rows.clone()).collect()
    }
}

/// `text` with `\r\n` and a lone `\r` made `\n`.
pub fn normalize(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_owned()
    }
}

pub fn rows(text: &str) -> u32 {
    text.matches('\n').count() as u32 + 1
}

/// The byte where each line starts, plus the end of the text: one more entry than lines.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.match_indices('\n')
            .map(|(at, _)| at + 1)
            .filter(|&at| at < text.len()),
    );
    if !text.is_empty() {
        starts.push(text.len());
    }
    starts
}

/// The byte where line `row` starts, or the end of the text past the last line.
fn line_start(text: &str, row: u32) -> usize {
    if row == 0 {
        return 0;
    }
    text.match_indices('\n')
        .nth(row as usize - 1)
        .map_or(text.len(), |(at, _)| at + 1)
}

/// The chars that count as part of a word in the word diff of `path`'s language, besides letters,
/// digits and `_`: the language's completion query chars (Zed's `config.toml` facts).
pub fn word_chars(path: &str) -> &'static [char] {
    match lang_for(path).map(|lang| lang.name) {
        Some("CSS") => &['-', '@'],
        Some("Markdown") => &['-'],
        _ => &[],
    }
}

/// The diff of `text` against `base`. No base (a new file): one hunk of every line.
pub fn diff(base: Option<&str>, text: &str, word_chars: &[char]) -> FileDiff {
    let new = normalize(text);
    let Some(base) = base else {
        let lines = line_starts(&new).len() as u32 - 1;
        return FileDiff {
            old: String::new(),
            new,
            hunks: vec![Hunk {
                rows: HunkRows {
                    old: 0..0,
                    new: 0..lines,
                },
                old_words: Vec::new(),
                new_words: Vec::new(),
            }],
        };
    };
    let old = normalize(base);
    // An emptied file is one line break: one hunk of all old lines, not a kept empty line
    // between two hunks.
    let hunks = if new == "\n" && old.ends_with('\n') && old.len() > 1 {
        let kept = &old[..old.len() - 1];
        let end = kept.matches('\n').count() as u32 + u32::from(!kept.ends_with('\n'));
        vec![Hunk {
            rows: HunkRows {
                old: 0..end,
                new: 0..0,
            },
            old_words: Vec::new(),
            new_words: Vec::new(),
        }]
    } else {
        line_hunks(&old, &new, word_chars)
    };
    FileDiff { old, new, hunks }
}

fn line_hunks(old: &str, new: &str, word_chars: &[char]) -> Vec<Hunk> {
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let old_starts = line_starts(old);
    let new_starts = line_starts(new);
    let bytes = |starts: &[usize], rows: &Range<u32>| {
        let at = |row: u32| starts[(row as usize).min(starts.len() - 1)];
        at(rows.start)..at(rows.end)
    };
    diff.hunks()
        .map(|hunk| {
            let lines = hunk.before.end - hunk.before.start;
            let words = !hunk.after.is_empty()
                && lines == hunk.after.end - hunk.after.start
                && lines <= MAX_WORD_DIFF_LINES;
            let (old_words, new_words) = if words {
                let old_bytes = bytes(&old_starts, &hunk.before);
                let new_bytes = bytes(&new_starts, &hunk.after);
                let (old_words, new_words) =
                    word_diff(&old[old_bytes], &new[new_bytes.clone()], word_chars);
                let at = new_bytes.start;
                let shift = |range: Range<usize>| range.start + at..range.end + at;
                (old_words, new_words.into_iter().map(shift).collect())
            } else {
                (Vec::new(), Vec::new())
            };
            Hunk {
                rows: HunkRows {
                    old: hunk.before,
                    new: hunk.after,
                },
                old_words,
                new_words,
            }
        })
        .collect()
}

/// The changed rows of two texts, `(old, new)` per change, with no shifting of the changes (the
/// ignore-whitespace base needs the plain line diff).
pub fn line_diff(old: &str, new: &str) -> Vec<(Range<u32>, Range<u32>)> {
    let input = InternedInput::new(old, new);
    Diff::compute(Algorithm::Histogram, &input)
        .hunks()
        .map(|hunk| (hunk.before, hunk.after))
        .collect()
}

/// What a char counts as when text is cut into tokens. Each punctuation char is its own class,
/// so only a repeat of the same char stays in one token.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Punct(char),
}

/// Runs of chars of one class: `==` is one token and `=>` two.
fn tokens<'a>(text: &'a str, word_chars: &[char]) -> Vec<&'a str> {
    let class_of = |c: char| {
        if c.is_alphanumeric() || c == '_' || word_chars.contains(&c) {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Punct(c)
        }
    };
    let mut cuts: Vec<usize> = text
        .char_indices()
        .zip(text.chars().skip(1))
        .filter(|((_, a), b)| class_of(*a) != class_of(*b))
        .map(|((at, a), _)| at + a.len_utf8())
        .collect();
    cuts.insert(0, 0);
    cuts.push(text.len());
    cuts.windows(2)
        .filter(|w| w[0] < w[1])
        .map(|w| &text[w[0]..w[1]])
        .collect()
}

/// The bytes where each token starts, plus the end.
fn token_starts(tokens: &[&str]) -> Vec<usize> {
    let mut at = 0;
    let mut starts = vec![0];
    for token in tokens {
        at += token.len();
        starts.push(at);
    }
    starts
}

/// A range joins the last one when they touch.
fn push_merged(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.is_empty() {
        return;
    }
    match ranges.last_mut() {
        Some(last) if last.end >= range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

/// The changed words of two texts, as byte ranges of each.
pub fn word_diff(
    old: &str,
    new: &str,
    word_chars: &[char],
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let old_tokens = tokens(old, word_chars);
    let new_tokens = tokens(new, word_chars);
    let mut input: InternedInput<&str> = InternedInput::default();
    input.update_before(old_tokens.iter().copied());
    input.update_after(new_tokens.iter().copied());
    let old_at = token_starts(&old_tokens);
    let new_at = token_starts(&new_tokens);
    let (mut old_ranges, mut new_ranges) = (Vec::new(), Vec::new());
    for hunk in Diff::compute(Algorithm::Histogram, &input).hunks() {
        let range =
            |at: &[usize], tokens: Range<u32>| at[tokens.start as usize]..at[tokens.end as usize];
        push_merged(&mut old_ranges, range(&old_at, hunk.before));
        push_merged(&mut new_ranges, range(&new_at, hunk.after));
    }
    (old_ranges, new_ranges)
}

/// How a row of text shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Added,
    Deleted,
}

/// One row of a side. `row` is 0-based in that side's text; a `Deleted` row of an inline diff is
/// a row of the old text. A `Spacer` keeps two sides level; a `Gap` stands for the unchanged rows
/// between two collapsed parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Line { row: u32, status: Status },
    Deleted { row: u32 },
    Spacer { rows: u32 },
    Gap { hidden: u32 },
}

/// The parts of a side that show, as inclusive row ranges: every row, or `CONTEXT_LINES` around
/// each change, with parts that touch joined.
pub fn parts(changes: &[Range<u32>], rows: u32, collapse: bool) -> Vec<RangeInclusive<u32>> {
    let last = rows.saturating_sub(1);
    if !collapse || changes.is_empty() {
        return vec![RangeInclusive::new(0, last)];
    }
    let mut parts: Vec<RangeInclusive<u32>> = Vec::new();
    for change in changes {
        let start = change.start.saturating_sub(CONTEXT_LINES).min(last);
        let end = (change.end + CONTEXT_LINES).min(last);
        match parts.last_mut() {
            Some(part) if start <= part.end() + 1 => {
                *part = *part.start()..=end.max(*part.end());
            }
            _ => parts.push(start..=end),
        }
    }
    parts
}

/// Walks the parts: a `Gap` between two, then `row` for each shown row.
fn walk(
    parts: &[RangeInclusive<u32>],
    out: &mut Vec<Row>,
    mut row: impl FnMut(u32, &mut Vec<Row>),
) {
    let mut last_end: Option<u32> = None;
    for part in parts {
        if let Some(end) = last_end {
            out.push(Row::Gap {
                hidden: part.start().saturating_sub(end + 1),
            });
        }
        for at in part.clone() {
            row(at, out);
        }
        last_end = Some(*part.end());
    }
}

/// Whether `row` is inside one of `changes`, which are in order; rows must come in order too.
struct ChangeCursor<'a> {
    changes: &'a [Range<u32>],
    next: usize,
}

impl ChangeCursor<'_> {
    fn holds(&mut self, row: u32) -> bool {
        while self
            .changes
            .get(self.next)
            .is_some_and(|change| change.end <= row)
        {
            self.next += 1;
        }
        self.changes
            .get(self.next)
            .is_some_and(|change| change.contains(&row))
    }
}

/// The rows of one side of a side by side diff: its lines, changed ones marked, with the empty
/// rows of the spacers above their row (or at the end, past the last row or outside the parts).
pub fn side_rows(
    hunks: &[HunkRows],
    side: Side,
    old_rows: u32,
    new_rows: u32,
    collapse: bool,
) -> Vec<Row> {
    let (rows, status) = match side {
        Side::Old => (old_rows, Status::Deleted),
        Side::New => (new_rows, Status::Added),
    };
    let changes: Vec<Range<u32>> = hunks
        .iter()
        .map(|hunk| match side {
            Side::Old => hunk.old.clone(),
            Side::New => hunk.new.clone(),
        })
        .collect();
    let mut empty: Vec<(u32, u32)> = spacers(hunks)
        .into_iter()
        .chain(end_spacer(hunks, old_rows, new_rows))
        .filter(|spacer| spacer.side == side)
        .map(|spacer| (spacer.before_row, spacer.rows))
        .collect();
    let mut cursor = ChangeCursor {
        changes: &changes,
        next: 0,
    };
    let mut out = Vec::new();
    walk(&parts(&changes, rows, collapse), &mut out, |row, out| {
        let here: u32 = empty
            .iter_mut()
            .filter(|(before, _)| *before == row)
            .map(|(_, rows)| std::mem::take(rows))
            .sum();
        if here > 0 {
            out.push(Row::Spacer { rows: here });
        }
        let status = if cursor.holds(row) {
            status
        } else {
            Status::Unchanged
        };
        out.push(Row::Line { row, status });
    });
    let rest: u32 = empty.iter().map(|(_, rows)| rows).sum();
    if rest > 0 {
        out.push(Row::Spacer { rows: rest });
    }
    out
}

/// The rows of an inline diff: the new lines, added ones marked, with the old lines of each hunk
/// as `Deleted` rows above the hunk's first new row.
pub fn inline_rows(hunks: &[HunkRows], new_rows: u32, collapse: bool) -> Vec<Row> {
    let changes: Vec<Range<u32>> = hunks.iter().map(|hunk| hunk.new.clone()).collect();
    let mut cursor = ChangeCursor {
        changes: &changes,
        next: 0,
    };
    let deleted = |hunk: &HunkRows, out: &mut Vec<Row>| {
        out.extend(hunk.old.clone().map(|row| Row::Deleted { row }));
    };
    let mut next = 0;
    let mut out = Vec::new();
    walk(
        &parts(&changes, new_rows, collapse),
        &mut out,
        |row, out| {
            while let Some(hunk) = hunks.get(next).filter(|hunk| hunk.new.start <= row) {
                if hunk.new.start == row {
                    deleted(hunk, out);
                }
                next += 1;
            }
            let status = if cursor.holds(row) {
                Status::Added
            } else {
                Status::Unchanged
            };
            out.push(Row::Line { row, status });
        },
    );
    for hunk in hunks.get(next..).unwrap_or_default() {
        deleted(hunk, &mut out);
    }
    out
}

#[cfg(test)]
#[path = "diff_model_tests.rs"]
mod tests;
