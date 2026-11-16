//! The text of one input (plan 6.3.2): a `String` that stores only `\n`, one selection, the mark
//! of an input method and the undo history. Plain Rust, no gpui, as a rules file.

use std::ops::Range;
use std::time::{Duration, Instant};

use super::minimal;

/// Edits of one kind closer than this join one undo step.
pub const GROUP_WITHIN: Duration = Duration::from_millis(300);

/// What an edit is, for the undo steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// Typed text: joins the typing before it within `GROUP_WITHIN`.
    Typing,
    /// Backspace and Delete: join each other within `GROUP_WITHIN`.
    Deleting,
    /// Text of an input method, from the first mark to the commit: one step.
    Composing,
    /// A paste: it never joins the step before it, but the edits that follow within
    /// `GROUP_WITHIN` join it, as Zed's undo does (T5).
    Paste,
    /// Cut, `set_text` and the like: always a step of its own.
    Other,
}

/// One replace: `range` of the text before it became `inserted`; `removed` was there.
#[derive(Clone, Debug)]
struct Change {
    start: usize,
    removed: String,
    inserted: String,
}

/// One replace as it was made: `removed` bytes from `start` of the text then became `inserted`
/// bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Splice {
    pub start: usize,
    pub removed: usize,
    pub inserted: usize,
}

#[derive(Clone, Debug)]
struct Step {
    changes: Vec<Change>,
    before: Selection,
    after: Selection,
    kind: EditKind,
    at: Instant,
    /// The next edit of the same kind may join it.
    open: bool,
}

/// A caret (`anchor == head`) or a selection from `anchor` to `head`, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Debug, Default)]
pub struct TextModel {
    text: String,
    selection: Selection,
    /// The x a run of Up and Down keys keeps, in px from the left of the text.
    pub goal_x: Option<f32>,
    marked: Option<Range<usize>>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Goes up by one on each change of the text, so a view knows when to lay it out again.
    version: u64,
    /// The replaces since `track_edits`, in the order they were made; None when no one follows
    /// them, or after `set_text`.
    log: Option<Vec<Splice>>,
}

/// `\r\n` and `\r` become `\n`.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

impl TextModel {
    /// A model that starts with `text`: the caret at the start and nothing to undo.
    pub fn with_text(text: &str) -> Self {
        TextModel {
            text: normalize(text),
            ..TextModel::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn head(&self) -> usize {
        self.selection.head
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn set_marked(&mut self, marked: Option<Range<usize>>) {
        self.marked = marked
            .map(|range| self.clip(range.start)..self.clip(range.end))
            .filter(|range| !range.is_empty());
    }

    /// Puts the selection from `anchor` to `head`, each on a char boundary inside the text.
    pub fn select(&mut self, anchor: usize, head: usize) {
        self.selection = Selection {
            anchor: self.clip(anchor),
            head: self.clip(head),
        };
    }

    pub fn move_to(&mut self, at: usize) {
        self.select(at, at);
    }

    /// Moves the head and keeps the anchor.
    pub fn select_to(&mut self, head: usize) {
        self.select(self.selection.anchor, head);
    }

    fn clip(&self, at: usize) -> usize {
        super::motion::floor_boundary(&self.text, at)
    }

    /// Replaces `range` with `text` (line ends normalised) and puts the caret after it. Returns
    /// whether the text changed.
    pub fn edit(&mut self, range: Range<usize>, text: &str, kind: EditKind, now: Instant) -> bool {
        let text = normalize(text);
        let range = self.clip(range.start)..self.clip(range.end.max(range.start));
        if range.is_empty() && text.is_empty() {
            return false;
        }
        let before = self.selection;
        let change = self.apply(range.start, range.end, &text);
        let caret = range.start + text.len();
        self.selection = Selection::caret(caret);
        self.record(vec![change], before, kind, now);
        true
    }

    /// Replaces the whole text as one step of its own; the caret goes to the end.
    pub fn set_text(&mut self, text: &str, now: Instant) -> bool {
        let text = normalize(text);
        if text == self.text {
            return false;
        }
        let before = self.selection;
        let change = self.apply(0, self.text.len(), &text);
        // A whole new text: what moved where is not known.
        self.log = None;
        self.selection = Selection::caret(self.text.len());
        self.marked = None;
        self.record(vec![change], before, EditKind::Other, now);
        true
    }

    /// Changes the text into `text` with the fewest edits, so the caret, the selection and the
    /// rows above an edit stay where they are. One step of its own.
    pub fn replace_minimal(&mut self, text: &str, now: Instant) -> bool {
        let text = normalize(text);
        if text == self.text {
            return false;
        }
        let edits = minimal::edits(&self.text, &text);
        let map = |at: usize| minimal::map_offset(&edits, at);
        let (anchor, head) = (map(self.selection.anchor), map(self.selection.head));
        self.edit_all(&edits, anchor, head, now);
        self.marked = None;
        true
    }

    /// Makes `edits` (front to back, apart, with ranges of the text before them) as one step of
    /// its own, then selects from `anchor` to `head` (offsets of the text after them).
    pub fn edit_all(&mut self, edits: &[minimal::Edit], anchor: usize, head: usize, now: Instant) {
        let before = self.selection;
        // Back to front, so the earlier offsets stay right.
        let mut changes: Vec<Change> = edits
            .iter()
            .rev()
            .map(|edit| self.apply(edit.range.start, edit.range.end, &edit.text))
            .collect();
        changes.reverse();
        self.select(anchor, head);
        // The changes are kept front to back with offsets of the text after the earlier ones.
        let mut shift: isize = 0;
        for change in &mut changes {
            change.start = (change.start as isize + shift) as usize;
            shift += change.inserted.len() as isize - change.removed.len() as isize;
        }
        self.record(changes, before, EditKind::Other, now);
    }

    /// Moves the text of `range` to `to` (an offset outside it), or copies it there, as one step;
    /// the text that went in is selected.
    pub fn drop_text(&mut self, range: Range<usize>, to: usize, copy: bool, now: Instant) -> bool {
        let range = self.clip(range.start)..self.clip(range.end);
        let to = self.clip(to);
        if range.is_empty() || (range.start..=range.end).contains(&to) {
            return false;
        }
        let moved = self.text[range.clone()].to_string();
        let before = self.selection;
        let mut changes = Vec::new();
        let at = if copy {
            changes.push(self.apply(to, to, &moved));
            to
        } else if to > range.end {
            changes.push(self.apply(range.start, range.end, ""));
            let at = to - moved.len();
            changes.push(self.apply(at, at, &moved));
            at
        } else {
            changes.push(self.apply(to, to, &moved));
            let start = range.start + moved.len();
            changes.push(self.apply(start, start + moved.len(), ""));
            to
        };
        self.selection = Selection {
            anchor: at,
            head: at + moved.len(),
        };
        self.record(changes, before, EditKind::Other, now);
        true
    }

    /// Starts a new list of the replaces made from now on.
    pub fn track_edits(&mut self) {
        self.log = Some(Vec::new());
    }

    /// The replaces since `track_edits`, and ends the list; None when they are not known.
    pub fn take_edits(&mut self) -> Option<Vec<Splice>> {
        self.log.take()
    }

    fn note(&mut self, start: usize, removed: usize, inserted: usize) {
        if let Some(log) = self.log.as_mut() {
            log.push(Splice {
                start,
                removed,
                inserted,
            });
        }
    }

    fn apply(&mut self, start: usize, end: usize, text: &str) -> Change {
        let removed = self.text[start..end].to_string();
        self.text.replace_range(start..end, text);
        self.note(start, end - start, text.len());
        // The mark moves with an edit before it and goes with an edit over it.
        self.marked = self.marked.take().and_then(|marked| {
            if end <= marked.start {
                let shift = text.len() as isize - (end - start) as isize;
                let moved = |at: usize| (at as isize + shift) as usize;
                Some(moved(marked.start)..moved(marked.end))
            } else if start >= marked.end {
                Some(marked)
            } else {
                None
            }
        });
        self.version += 1;
        self.goal_x = None;
        Change {
            start,
            removed,
            inserted: text.to_string(),
        }
    }

    fn record(&mut self, changes: Vec<Change>, before: Selection, kind: EditKind, now: Instant) {
        self.redo.clear();
        let after = self.selection;
        let within = |last: &Step| now.saturating_duration_since(last.at) < GROUP_WITHIN;
        if let Some(last) = self.undo.last_mut()
            && last.open
            && match (last.kind, kind) {
                (_, EditKind::Other | EditKind::Paste) => false,
                (EditKind::Composing, EditKind::Composing) => true,
                (EditKind::Paste, EditKind::Typing | EditKind::Deleting) => within(last),
                (before, now_kind) => before == now_kind && within(last),
            }
        {
            last.changes.extend(changes);
            last.after = after;
            last.at = now;
            return;
        }
        if let Some(last) = self.undo.last_mut() {
            last.open = false;
        }
        self.undo.push(Step {
            changes,
            before,
            after,
            kind,
            at: now,
            open: kind != EditKind::Other,
        });
    }

    /// The next edit starts a step of its own (the commit of an input method ends its step).
    pub fn seal(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.open = false;
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        for change in step.changes.iter().rev() {
            let end = change.start + change.inserted.len();
            self.text.replace_range(change.start..end, &change.removed);
            self.note(change.start, change.inserted.len(), change.removed.len());
        }
        self.after_history(step.before);
        self.redo.push(Step {
            open: false,
            ..step
        });
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        for change in &step.changes {
            let end = change.start + change.removed.len();
            self.text.replace_range(change.start..end, &change.inserted);
            self.note(change.start, change.removed.len(), change.inserted.len());
        }
        self.after_history(step.after);
        self.undo.push(step);
        true
    }

    fn after_history(&mut self, selection: Selection) {
        self.version += 1;
        self.goal_x = None;
        self.marked = None;
        self.selection = Selection {
            anchor: self.clip(selection.anchor),
            head: self.clip(selection.head),
        };
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
