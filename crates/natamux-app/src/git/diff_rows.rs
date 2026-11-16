//! What one side of the diff view shows (plan 6.5, S6.2 part 2), as plain data: the rows of
//! `diff_model::side_rows` or `inline_rows` turned into the side's text (each shown line once), a
//! number, a status and a hunk per line, the empty rows above lines, the markers of hunks with no
//! lines on the side, and the changed words as ranges of the text. `text_view/diff_side.rs` draws
//! it.

use std::ops::Range;
use std::rc::Rc;

use super::diff_model::{FileDiff, Row, Status};
use super::diff_rules::HunkRows;

/// What a hunk changes: lines added, lines deleted, or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Deleted,
    Modified,
}

impl Kind {
    fn of(rows: &HunkRows) -> Self {
        match (rows.old.is_empty(), rows.new.is_empty()) {
            (true, _) => Kind::Added,
            (_, true) => Kind::Deleted,
            _ => Kind::Modified,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// Empty rows that keep the two sides level.
    Spacer,
    /// The unchanged lines between two collapsed parts.
    Gap,
    /// A review comment, drawn over its rows by the diff view.
    Comment,
}

/// Rows put in after `side_rows`: a comment, or the empty rows that match it on the other side.
/// `early` rows (under the line above) come before the spacers and gaps above `line`, the
/// others after them, as Zed's editor ordered its blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extra {
    pub line: usize,
    pub rows: usize,
    pub kind: BlockKind,
    pub early: bool,
}

/// Empty rows above line `line` of the side's text (`lines.len()`: below the last line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub line: usize,
    pub rows: usize,
    pub kind: BlockKind,
}

/// A line of the side's text: the file it comes from and its row there (0-based), the number the
/// gutter shows, how it changed, and the hunk it is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SideLine {
    pub old: bool,
    pub row: u32,
    pub number: Option<u32>,
    pub status: Status,
    pub hunk: Option<usize>,
}

/// What one side shows: its text (the shown lines, each once), a `SideLine` per line, the empty
/// rows, and the marks.
#[derive(Clone, Debug, PartialEq)]
pub struct SideRows {
    pub text: String,
    pub lines: Vec<SideLine>,
    /// The offset in `text` where each line starts.
    pub line_starts: Vec<usize>,
    pub blocks: Vec<Block>,
    /// The empty rows above each line, plus one entry for below the last line.
    pub above: Rc<[usize]>,
    /// The kind of each hunk of the diff.
    pub kinds: Vec<Kind>,
    /// A marker at the top of a line (or below the last line) for each hunk with no rows on this
    /// side, with the hunk: only on the working tree's side of side by side.
    pub markers: Vec<(usize, usize)>,
    /// The changed words, as ranges of `text`, deleted or added.
    pub words: Vec<(Range<usize>, Status)>,
    /// Per file (the last commit's, the working tree's), the line each of its rows shows on.
    pub shown: [Vec<Option<usize>>; 2],
    /// The hunks of an inline diff whose deleted lines are closed: their lines show as unchanged
    /// (no box, no words, no buttons), only their strip stays, as Zed's collapsed hunks.
    pub closed: Vec<usize>,
}

impl SideRows {
    /// The line of the side's text that shows row `row` of the last commit's file (`old`) or of
    /// the working tree's; None when it does not show.
    pub fn line_of(&self, old: bool, row: u32) -> Option<usize> {
        self.shown[usize::from(!old)]
            .get(row as usize)
            .copied()
            .flatten()
    }

    /// Closes `closed` hunks of an inline diff (their deleted lines are left out already): their
    /// lines keep the hunk but show as unchanged, and their words go.
    pub fn close(&mut self, closed: &[usize]) {
        self.closed = closed.to_vec();
        if closed.is_empty() {
            return;
        }
        let is_closed = |line: &SideLine| line.hunk.is_some_and(|hunk| closed.contains(&hunk));
        for line in &mut self.lines {
            if is_closed(&*line) {
                line.status = Status::Unchanged;
            }
        }
        let (lines, starts) = (&self.lines, &self.line_starts);
        self.words.retain(|(range, _)| {
            let at = starts.partition_point(|&start| start <= range.start);
            lines
                .get(at.saturating_sub(1))
                .is_none_or(|line| !is_closed(line))
        });
    }

    /// The line that holds offset `at` of the text.
    pub fn line_at(&self, at: usize) -> usize {
        self.line_starts
            .partition_point(|&start| start <= at)
            .saturating_sub(1)
    }

    /// Puts `extra` in among the blocks, and gives the index in `blocks` of each.
    pub fn add_blocks(&mut self, extra: &[Extra]) -> Vec<usize> {
        let mut blocks = Vec::with_capacity(self.blocks.len() + extra.len());
        let mut placed = vec![0; extra.len()];
        let mut old = self.blocks.iter().peekable();
        for line in 0..=self.lines.len() {
            let mut put = |early: bool, blocks: &mut Vec<Block>| {
                for (index, more) in extra.iter().enumerate() {
                    if more.line == line && more.early == early {
                        placed[index] = blocks.len();
                        blocks.push(Block {
                            line,
                            rows: more.rows,
                            kind: more.kind,
                        });
                    }
                }
            };
            put(true, &mut blocks);
            while let Some(block) = old.next_if(|block| block.line == line) {
                blocks.push(*block);
            }
            put(false, &mut blocks);
        }
        self.blocks = blocks;
        let mut above = vec![0; self.lines.len() + 1];
        for block in &self.blocks {
            above[block.line] += block.rows;
        }
        self.above = Rc::from(above);
        placed
    }

    /// The line block `index` is above, and the rows of the blocks before it there.
    pub fn block_offset(&self, index: usize) -> Option<(usize, usize)> {
        let block = self.blocks.get(index)?;
        let before = self.blocks[..index]
            .iter()
            .filter(|other| other.line == block.line)
            .map(|other| other.rows)
            .sum();
        Some((block.line, before))
    }

    pub fn empty() -> Self {
        SideRows {
            text: String::new(),
            lines: Vec::new(),
            line_starts: vec![0],
            blocks: Vec::new(),
            above: Rc::from(vec![0, 0]),
            kinds: Vec::new(),
            markers: Vec::new(),
            words: Vec::new(),
            shown: [Vec::new(), Vec::new()],
            closed: Vec::new(),
        }
    }
}

/// The ranges of the lines of `text`, line breaks left out: one per row, the empty one after a
/// final line break included.
pub fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut start = 0;
    let mut lines = Vec::new();
    for (at, _) in text.match_indices('\n') {
        lines.push(start..at);
        start = at + 1;
    }
    lines.push(start..text.len());
    lines
}

/// The row of `lines` that holds byte `at`.
fn row_of(lines: &[Range<usize>], at: usize) -> usize {
    lines
        .partition_point(|line| line.start <= at)
        .saturating_sub(1)
}

/// The hunk a row of one side is in; rows must come in order.
struct HunkCursor<'a> {
    hunks: &'a [HunkRows],
    old: bool,
    next: usize,
}

impl HunkCursor<'_> {
    fn find(&mut self, row: u32) -> Option<usize> {
        let old = self.old;
        let rows = move |hunk: &HunkRows| {
            if old {
                hunk.old.clone()
            } else {
                hunk.new.clone()
            }
        };
        while self
            .hunks
            .get(self.next)
            .is_some_and(|hunk| rows(hunk).end <= row)
        {
            self.next += 1;
        }
        let hunk = self.hunks.get(self.next)?;
        rows(hunk).contains(&row).then_some(self.next)
    }
}

/// What a side shows for `rows` of `diff`: the rows of the working tree's side (`new_side`, also
/// inline) or of the last commit's side of side by side.
pub fn side_rows(diff: &FileDiff, rows: &[Row], new_side: bool) -> SideRows {
    let sources = [line_ranges(&diff.old), line_ranges(&diff.new)];
    let hunks = diff.hunk_rows();
    let mut side = SideRows::empty();
    side.kinds = hunks.iter().map(Kind::of).collect();
    let shown = add_lines(&mut side, diff, &sources, rows, new_side);
    let mut above = vec![0; side.lines.len() + 1];
    for block in &side.blocks {
        above[block.line] += block.rows;
    }
    side.above = Rc::from(above);
    // The working tree's side marks where a hunk's deleted lines were when none of them shows:
    // always side by side, and inline when the hunk is closed.
    if new_side {
        for (index, hunk) in hunks.iter().enumerate() {
            let old_shows = hunk
                .old
                .clone()
                .any(|row| shown[0].get(row as usize).is_some_and(Option::is_some));
            if hunk.new.is_empty() && !hunk.old.is_empty() && !old_shows {
                let line = side
                    .lines
                    .iter()
                    .position(|line| !line.old && line.row >= hunk.new.start)
                    .unwrap_or(side.lines.len());
                side.markers.push((line, index));
            }
        }
    }
    add_words(&mut side, diff, &sources, &shown);
    side.shown = shown;
    side
}

/// Puts the lines and the empty rows of `rows` into `side`. Gives, per file, the line of the
/// side's text each of its rows shows on.
fn add_lines(
    side: &mut SideRows,
    diff: &FileDiff,
    sources: &[Vec<Range<usize>>; 2],
    rows: &[Row],
    new_side: bool,
) -> [Vec<Option<usize>>; 2] {
    let texts = [diff.old.as_str(), diff.new.as_str()];
    let hunks = diff.hunk_rows();
    let mut cursors = [true, false].map(|old| HunkCursor {
        hunks: &hunks,
        old,
        next: 0,
    });
    let mut shown = [vec![None; sources[0].len()], vec![None; sources[1].len()]];
    side.line_starts.clear();
    for row in rows {
        let (old, row, status) = match *row {
            Row::Line { row, status } => (!new_side, row, status),
            Row::Deleted { row } => (true, row, Status::Deleted),
            Row::Spacer { rows } => {
                side.blocks.push(Block {
                    line: side.lines.len(),
                    rows: rows as usize,
                    kind: BlockKind::Spacer,
                });
                continue;
            }
            Row::Gap { .. } => {
                side.blocks.push(Block {
                    line: side.lines.len(),
                    rows: 1,
                    kind: BlockKind::Gap,
                });
                continue;
            }
        };
        let source = usize::from(!old);
        let Some(range) = sources[source].get(row as usize) else {
            continue;
        };
        if !side.lines.is_empty() {
            side.text.push('\n');
        }
        side.line_starts.push(side.text.len());
        side.text.push_str(&texts[source][range.clone()]);
        if let Some(slot) = shown[source].get_mut(row as usize) {
            *slot = Some(side.lines.len());
        }
        let hunk = if status == Status::Unchanged {
            None
        } else {
            cursors[source].find(row)
        };
        side.lines.push(SideLine {
            old,
            row,
            number: (status != Status::Deleted).then_some(row + 1),
            status,
            hunk,
        });
    }
    if side.line_starts.is_empty() {
        side.line_starts.push(0);
    }
    shown
}

/// Puts the changed words of `diff` that show into `side`, cut at line ends.
fn add_words(
    side: &mut SideRows,
    diff: &FileDiff,
    sources: &[Vec<Range<usize>>; 2],
    shown: &[Vec<Option<usize>>; 2],
) {
    for hunk in &diff.hunks {
        let old_start = diff.old_start(hunk);
        let words = hunk
            .old_words
            .iter()
            .map(|range| (range.start + old_start..range.end + old_start, 0))
            .chain(hunk.new_words.iter().map(|range| (range.clone(), 1)));
        for (range, source) in words {
            let status = if source == 0 {
                Status::Deleted
            } else {
                Status::Added
            };
            let lines = &sources[source];
            for row in row_of(lines, range.start)..=row_of(lines, range.end.saturating_sub(1)) {
                let (Some(bytes), Some(Some(line))) = (lines.get(row), shown[source].get(row))
                else {
                    continue;
                };
                let (start, end) = (range.start.max(bytes.start), range.end.min(bytes.end));
                if start < end {
                    let at = side.line_starts[*line];
                    side.words
                        .push((at + start - bytes.start..at + end - bytes.start, status));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "diff_rows_tests.rs"]
mod tests;
