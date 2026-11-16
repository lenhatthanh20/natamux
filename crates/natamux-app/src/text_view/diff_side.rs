//! One side of the own diff view (plan 6.5, S6.2): a read-only code view whose text is the rows
//! the side shows (`diff_model::side_rows` or `inline_rows`), with:
//! - the line numbers of the side's file, none on deleted lines, added lines in the added colour;
//! - empty rows: the spacers that keep the two sides level (as tall as their rows, T-0230), one
//!   row with a line in its middle for the unchanged lines between two collapsed parts, and the
//!   rows of the review comments, which the diff view draws over them;
//! - the rows of each hunk in a box: the hollow background of their kind with a line at its top
//!   and its bottom, a strip in the gutter in the colour of the hunk's kind, and a marker where a
//!   hunk with no rows on this side lies;
//! - the changed words of small hunks;
//! - the syntax colours of each line from the parse of its own file, drawn at once and again when
//!   a parse ends (advisor A5). The deleted lines of an inline diff stay plain, as the B2 shots
//!   show them (`changes-diff-focused`, `workspace-changes-ask`);
//! - the matching brackets, found in the parse of the caret's own file;
//! - one scroll for the two sides of a side by side diff.
//!
//! The look values were measured on the `changes-diff-*` shots of B2. `git/diff_view.rs` uses it.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, AppContext as _, Context, Entity, EntityId, FocusHandle, Pixels, Point, SharedString,
    Size, Subscription, Task, TextRun, WeakEntity, Window, px,
};
use theme::ActiveTheme;

use super::code_runs::styled_runs;
use super::code_view::{Code, CodeView, brackets_around};
use super::element::Layout;
use super::highlight::{Highlights, PARSE_BUDGET, Syntax};
use super::outline::outline;
use super::text_box::TextBox;
use super::text_input::TextInput;
use crate::files::outline_rules::OutlineRow;
use crate::git::code_target::{BlameTarget, FindTarget, RowMenu};
use crate::git::diff_model::FileDiff;
#[cfg(test)]
use crate::git::diff_model::Row;
#[cfg(test)]
use crate::git::diff_rows::side_rows;
use crate::git::diff_rows::{SideRows, line_ranges};
use crate::git::languages::{Lang, lang_for};

/// One file of a diff: its text, its lines, and its parse.
struct Source {
    text: Arc<str>,
    lines: Vec<Range<usize>>,
    syntax: Option<Syntax>,
}

/// The syntax colours of the last query: the colour count, the theme and the lines they are of.
#[derive(Default)]
struct RunCache {
    colors: u64,
    theme: SharedString,
    lines: Option<Range<usize>>,
    spans: Vec<(Range<usize>, String)>,
}

/// What a code view has more as a diff side.
pub(super) struct DiffDecor {
    pub(super) rows: SideRows,
    /// The working tree's side (also the only side of an inline diff), or the last commit's.
    new_side: bool,
    /// The last commit's file and the working tree's.
    sources: [Option<Source>; 2],
    cache: RefCell<RunCache>,
    /// The other side of a side by side diff, which scrolls with this one.
    peer: Option<WeakEntity<TextInput>>,
    /// The scroll this side last had after a draw or a sync.
    synced: Option<Point<Pixels>>,
    /// The scroll the peer last gave this side. When the side clamps it, the clamped value is not
    /// sent back, so the taller side can still scroll to its end (wrap on, D17).
    given: Option<Point<Pixels>>,
    /// The scroll this side was last laid out at, for the tests.
    #[cfg(test)]
    pub(super) drawn: Option<Point<Pixels>>,
    _parses: Vec<Task<()>>,
}

impl DiffDecor {
    fn new() -> Self {
        DiffDecor {
            rows: SideRows::empty(),
            new_side: true,
            sources: [None, None],
            cache: RefCell::default(),
            peer: None,
            synced: None,
            given: None,
            #[cfg(test)]
            drawn: None,
            _parses: Vec::new(),
        }
    }

    /// The largest line number the gutter shows.
    pub(super) fn widest_number(&self) -> usize {
        self.rows
            .lines
            .iter()
            .filter_map(|line| line.number)
            .max()
            .unwrap_or(1) as usize
    }

    /// Whether line `index` comes from the file of this side: not a deleted line of an inline
    /// diff, which comes from the last commit.
    fn own_line(&self, index: usize) -> bool {
        self.rows
            .lines
            .get(index)
            .is_some_and(|line| line.old != self.new_side)
    }

    /// The row in the side's file of line `index` of the text; None for a deleted line of an
    /// inline diff (it is a row of the other file).
    pub(super) fn file_row(&self, index: usize) -> Option<u32> {
        let line = self.rows.lines.get(index)?;
        self.own_line(index).then_some(line.row)
    }

    /// The side's own file: the working tree's or the last commit's.
    fn own_source(&self) -> Option<&Source> {
        self.sources.get(usize::from(self.new_side))?.as_ref()
    }
}

impl TextInput {
    /// The syntax colours of a diff side, from `base`: each line in its own file's colours. Only
    /// `lines` are coloured (every line for None). None for every other view.
    pub(super) fn diff_runs(
        &self,
        base: &TextRun,
        len: usize,
        lines: Option<Range<usize>>,
        cx: &App,
    ) -> Option<Vec<TextRun>> {
        let code = self.code.as_ref()?;
        let diff = code.diff.as_ref()?;
        let text = self.model.text();
        if text.len() != len {
            return None;
        }
        let theme_name = cx.theme().name.clone();
        let theme = cx.theme().syntax();
        let mut cache = diff.cache.borrow_mut();
        if cache.colors != code.colors || cache.theme != theme_name || cache.lines != lines {
            let keys: Vec<Option<Vec<Option<String>>>> = diff
                .sources
                .iter()
                .map(|source| {
                    let syntax = source.as_ref()?.syntax.as_ref()?;
                    let highlights = Highlights::of(syntax.lang()).ok()?;
                    Some(highlights.theme_keys(|key| theme.style_for_name(key).is_some()))
                })
                .collect();
            let mut spans = Vec::new();
            let shown = lines.clone().unwrap_or(0..diff.rows.lines.len());
            for index in shown {
                let Some(line) = diff.rows.lines.get(index) else {
                    break;
                };
                // The deleted lines of an inline diff stay plain (see the module comment).
                if line.old && diff.new_side {
                    continue;
                }
                let side = usize::from(!line.old);
                let (Some(Some(source)), Some(Some(keys))) =
                    (diff.sources.get(side), keys.get(side))
                else {
                    continue;
                };
                let Some(syntax) = source.syntax.as_ref() else {
                    continue;
                };
                let (Some(tree), Ok(highlights)) = (syntax.tree(), Highlights::of(syntax.lang()))
                else {
                    continue;
                };
                let (Some(bytes), Some(&at)) = (
                    source.lines.get(line.row as usize),
                    diff.rows.line_starts.get(index),
                ) else {
                    continue;
                };
                for (range, key) in highlights.runs(tree, &source.text, bytes.clone(), keys) {
                    let (start, end) = (range.start.max(bytes.start), range.end.min(bytes.end));
                    if start < end {
                        spans.push((at + start - bytes.start..at + end - bytes.start, key));
                    }
                }
            }
            *cache = RunCache {
                colors: code.colors,
                theme: theme_name,
                lines,
                spans,
            };
        }
        let style_for = |key: &str| theme.style_for_name(key).unwrap_or_default();
        Some(styled_runs(&cache.spans, base, text, style_for))
    }

    /// The matching brackets of a diff side, looked for in the parse of the caret's own file and
    /// shown where their lines show. None for every other view.
    pub(super) fn diff_brackets(&self) -> Option<Vec<Range<usize>>> {
        let code = self.code.as_ref()?;
        let diff = code.diff.as_ref()?;
        let selection = self.model.selection();
        let (Some(typing), true) = (code.typing(), selection.is_empty()) else {
            return Some(Vec::new());
        };
        let at = selection.head;
        let index = diff.rows.line_at(at);
        let (Some(line), Some(&start)) = (diff.rows.lines.get(index), diff.rows.line_starts.get(index))
        else {
            return Some(Vec::new());
        };
        if !diff.own_line(index) {
            return Some(Vec::new());
        }
        let Some(source) = diff.sources.get(usize::from(!line.old)).and_then(Option::as_ref)
        else {
            return Some(Vec::new());
        };
        let (Some(tree), Some(bytes)) = (
            source.syntax.as_ref().and_then(Syntax::tree),
            source.lines.get(line.row as usize),
        ) else {
            return Some(Vec::new());
        };
        let found = brackets_around(typing, tree, &source.text, bytes.start + (at - start));
        // Back to offsets of the side's text; a bracket on a line that does not show is left out.
        let shown = found
            .into_iter()
            .filter_map(|range| {
                let row = source
                    .lines
                    .partition_point(|bytes| bytes.start <= range.start)
                    .saturating_sub(1);
                let bytes = source.lines.get(row)?;
                let index = diff.rows.line_of(line.old, row as u32)?;
                let start = diff.rows.line_starts.get(index)? + (range.start - bytes.start);
                Some(start..start + range.len())
            })
            .collect::<Vec<_>>();
        // Both or none, as a pair.
        Some(if shown.len() == 2 { shown } else { Vec::new() })
    }

    /// What a press at `position` hits in a diff side's gutter: a line number, or the strip of a
    /// changed line. None elsewhere and in every other view.
    pub(super) fn diff_gutter_press(
        &mut self,
        position: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> Option<GutterPress> {
        self.code.as_ref()?.diff.as_ref()?;
        if !self.in_gutter(position) {
            return None;
        }
        let bounds = self.last_bounds?;
        let laid = self.layout(window, cx);
        let line_height = laid.line_height.max(px(1.));
        let y = position.y - bounds.top() + self.scroll.y;
        if y < px(0.) {
            return None;
        }
        let diff = self.code.as_ref()?.diff.as_ref()?;
        let x = position.x - bounds.left();
        if super::diff_paint::closed_marker_at(&laid, &diff.rows, x, y).is_some() {
            return Some(GutterPress::Strip);
        }
        let row = (y / line_height) as usize;
        let index = laid.line_index_of_row(row);
        let laid_line = laid.lines.get(index)?;
        if row >= laid_line.first_row + laid_line.row_starts.len() {
            return None;
        }
        let line = diff.rows.lines.get(index)?;
        let strip = (line_height * super::diff_paint::STRIP).floor();
        // A changed line, or a line of a closed hunk (the only unchanged lines with a hunk).
        if line.hunk.is_some() && x < strip {
            return Some(GutterPress::Strip);
        }
        let number = line.number?.to_string();
        let run = TextRun {
            len: number.len(),
            font: laid.key.font.clone(),
            color: gpui::Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = window
            .text_system()
            .shape_line(number.into(), laid.key.font_size, &[run], None)
            .width;
        let right = bounds.left() + laid.key.gutter - laid.key.gutter_pad;
        (position.x >= right - width && position.x < right).then_some(GutterPress::Number)
    }

    /// After a scroll of a diff side: the other side takes the same scroll, x and y, as both have
    /// the same rows. `before` is the scroll before this draw clamped it.
    pub(super) fn sync_peer(&mut self, before: Point<Pixels>, cx: &mut Context<Self>) {
        let scroll = self.scroll;
        let Some(diff) = self.code.as_mut().and_then(|code| code.diff.as_mut()) else {
            return;
        };
        #[cfg(test)]
        {
            diff.drawn = Some(scroll);
        }
        if diff.given.take() == Some(before) {
            diff.synced = Some(scroll);
            return;
        }
        if diff.synced == Some(scroll) {
            return;
        }
        diff.synced = Some(scroll);
        let Some(peer) = diff.peer.as_ref().and_then(WeakEntity::upgrade) else {
            return;
        };
        let moved = peer.update(cx, |peer, _| {
            let moved = peer.scroll != scroll;
            if let Some(diff) = peer.code.as_mut().and_then(|code| code.diff.as_mut()) {
                diff.given = Some(scroll);
            }
            peer.scroll = scroll;
            moved
        });
        // This runs in a draw, where gpui keeps a notify from drawing the window again; the peer
        // may already be laid out at its old scroll, so ask for its new draw after this frame.
        if moved {
            cx.defer(move |cx| peer.update(cx, |_, cx| cx.notify()));
        }
    }

    /// Selects `range` and scrolls its start into view: its row at the top, or in the middle as
    /// Zed's editor centred it (the top row is that row less half the other rows in view,
    /// rounded down). Before the first draw the caret is only kept in view.
    pub(super) fn select_and_reveal(
        &mut self,
        range: Range<usize>,
        reveal: Reveal,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.model.select(range.start, range.end);
        self.changed(false, cx);
        if reveal == Reveal::No {
            self.no_click_scroll();
            return;
        }
        let laid = self.layout(window, cx);
        if let Some(bounds) = self.last_bounds {
            self.autoscroll = false;
            let line_height = laid.line_height.max(px(1.));
            let row = laid.position(self.to_display(range.start), false).y / line_height;
            let above = match reveal {
                Reveal::Center => {
                    let shown = bounds.size.height / line_height;
                    ((shown - 1.) / 2.).floor().max(0.)
                }
                _ => 0.,
            };
            self.scroll.y = line_height * (row.round() - above).max(0.);
            self.clamp_scroll(&laid, bounds);
        }
        cx.notify();
    }
}

/// What a press in a diff side's gutter hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GutterPress {
    /// A line number: Zed's editor asked to open the line's excerpt there, which does nothing
    /// here (T-MOUSE-1).
    Number,
    /// The strip of a changed line: an inline diff opens or closes the change (`diff_view.rs`).
    Strip,
}

/// How a selection made by code scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// No scroll.
    No,
    /// The row of its start goes to the top (Zed's `Autoscroll::top_relative(0)`).
    Top,
    /// The row of its start goes to the middle (Zed's `Autoscroll::center`).
    Center,
}

/// Where a side's rows were last laid out, for what the diff view draws over it. Tops are from the
/// side's top edge, scroll included.
pub struct Place {
    laid: Rc<Layout>,
    scroll: Point<Pixels>,
    /// The side's top left in the window, and its size.
    pub origin: Point<Pixels>,
    pub size: Size<Pixels>,
    /// The room the scrollbar keeps at the right.
    pub room: Pixels,
}

impl Place {
    pub fn line_height(&self) -> Pixels {
        self.laid.line_height
    }

    pub fn scroll(&self) -> Point<Pixels> {
        self.scroll
    }

    /// The width of the gutter: the text starts right of it.
    pub fn gutter(&self) -> Pixels {
        self.laid.key.gutter
    }

    fn row_top(&self, row: usize) -> Pixels {
        self.laid.line_height * row as f32 - self.scroll.y
    }

    /// The top of line `index` of the text, and the rows it wraps into.
    pub fn line(&self, index: usize) -> Option<(Pixels, usize)> {
        let line = self.laid.lines.get(index)?;
        Some((self.row_top(line.first_row), line.row_starts.len()))
    }

    /// The top of the empty rows above line `line` (`lines.len()`: below the last line), `offset`
    /// rows down into them.
    pub fn block_top(&self, line: usize, offset: usize) -> Pixels {
        self.row_top(self.laid.blocks_top(line) + offset)
    }

    /// The first shown row of line `index`.
    #[cfg(test)]
    pub fn first_row(&self, index: usize) -> Option<usize> {
        Some(self.laid.lines.get(index)?.first_row)
    }

    /// The shown rows, empty rows included.
    #[cfg(test)]
    pub fn rows(&self) -> usize {
        self.laid.rows
    }

    /// The width of a change's strip at the gutter's left edge.
    pub fn strip(&self) -> Pixels {
        (self.laid.line_height * super::diff_paint::STRIP).floor()
    }

    /// The closed hunk whose gutter marker is at `at` from the side's top left.
    pub fn closed_marker_at(&self, rows: &SideRows, at: Point<Pixels>) -> Option<usize> {
        super::diff_paint::closed_marker_at(&self.laid, rows, at.x, at.y + self.scroll.y)
    }

    /// The line whose rows are at `y` from the side's top; None on an empty row.
    pub fn line_at(&self, y: Pixels) -> Option<usize> {
        let at = y + self.scroll.y;
        if at < px(0.) {
            return None;
        }
        let row = (at / self.laid.line_height.max(px(1.))) as usize;
        let index = self.laid.line_index_of_row(row);
        let line = self.laid.lines.get(index)?;
        (row >= line.first_row && row < line.first_row + line.row_starts.len()).then_some(index)
    }
}

/// One side of a diff: build it, show rows in it, then hand its `TextBox` to the owner.
#[derive(Clone)]
pub struct DiffSide {
    input: Entity<TextInput>,
}

impl DiffSide {
    /// An empty read-only side with line numbers, indent guides and the line of the caret
    /// painted, as the old diff's editors had them.
    pub fn new(wrap: bool, window: &mut Window, cx: &mut App) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::full(true, true, window, cx);
            let mut code = Code::new(None, true);
            code.gutter = true;
            code.wrap = wrap;
            code.diff = Some(DiffDecor::new());
            input.code = Some(code);
            input
        });
        Self { input }
    }

    /// The two sides of a side by side diff scroll as one.
    pub fn pair(&self, other: &DiffSide, cx: &mut App) {
        for (side, peer) in [(self, other), (other, self)] {
            let peer = peer.input.downgrade();
            side.input.update(cx, |input, _| {
                if let Some(diff) = input.code.as_mut().and_then(|code| code.diff.as_mut()) {
                    diff.peer = Some(peer);
                }
            });
        }
    }

    /// Shows `rows` of `diff` (see `side_rows`), coloured in the language of `path`.
    #[cfg(test)]
    pub fn show(&self, path: &str, diff: &FileDiff, rows: &[Row], new_side: bool, cx: &mut App) {
        self.show_rows(path, diff, side_rows(diff, rows, new_side), new_side, cx);
    }

    /// Shows `side`, made from `diff`. A newer read of the same file changes only the lines that
    /// differ, so the caret stays; the same two files keep their parse.
    pub fn show_rows(
        &self,
        path: &str,
        diff: &FileDiff,
        side: SideRows,
        new_side: bool,
        cx: &mut App,
    ) {
        let lang = lang_for(path);
        let texts = [diff.old.as_str(), diff.new.as_str()];
        self.input.update(cx, |input, cx| {
            let now = input.now(cx);
            if input.model.text().is_empty() {
                // Not a new model: its version would start again at 0, and the caches kept by
                // version (guides, outline) would keep showing the empty text.
                input.model.set_text(&side.text, now);
                input.model.move_to(0);
            } else {
                input.model.replace_minimal(&side.text, now);
            }
            input.model.forget_history();
            let same_lang = input
                .code
                .as_ref()
                .is_some_and(|code| code.lang.map(|lang| lang.name) == lang.map(|lang| lang.name));
            let kept = same_lang
                && input
                    .code
                    .as_ref()
                    .and_then(|code| code.diff.as_ref())
                    .is_some_and(|decor| {
                        decor.sources.iter().zip(texts).all(|(source, text)| {
                            source.as_ref().is_some_and(|source| *source.text == *text)
                        })
                    });
            let parsed = (!kept).then(|| parse_sources(texts, lang, cx));
            if let Some(code) = input.code.as_mut() {
                code.lang = lang;
                code.colors += 1;
                if let Some(decor) = code.diff.as_mut() {
                    decor.rows = side;
                    decor.new_side = new_side;
                    if let Some((sources, parses)) = parsed {
                        decor.sources = sources;
                        decor._parses = parses;
                    }
                }
            }
            input.sync_code(cx);
            input.refresh_brackets();
            cx.notify();
        });
    }

    /// Both files of the shown diff are parsed (or have no language), so every line has its
    /// colours; a `--shot` waits for it.
    pub fn parsed(&self, cx: &App) -> bool {
        let input = self.input.read(cx);
        let Some(diff) = input.code.as_ref().and_then(|code| code.diff.as_ref()) else {
            return false;
        };
        diff.sources.iter().flatten().all(|source| {
            source
                .syntax
                .as_ref()
                .is_none_or(|syntax| syntax.tree().is_some() && syntax.is_done())
        })
    }

    /// The working tree's file is parsed.
    pub fn new_parsed(&self, cx: &App) -> bool {
        let input = self.input.read(cx);
        let source = input
            .code
            .as_ref()
            .and_then(|code| code.diff.as_ref())
            .and_then(|diff| diff.sources[1].as_ref());
        source.is_some_and(|source| {
            source
                .syntax
                .as_ref()
                .is_some_and(|syntax| syntax.tree().is_some() && syntax.is_done())
        })
    }

    pub fn text_box(&self) -> TextBox {
        TextBox::of_input(&self.input)
    }

    #[cfg(test)]
    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    /// Calls `changed` each time the side's box tells it changed (a caret move among others).
    pub fn observe<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut changed: impl FnMut(&mut T, &mut Context<T>) + 'static,
    ) -> Subscription {
        cx.observe(&self.input, move |this, _, cx| changed(this, cx))
    }

    pub fn id(&self) -> EntityId {
        self.input.entity_id()
    }

    /// What the side shows now.
    pub fn rows<R>(&self, cx: &App, read: impl FnOnce(&SideRows) -> R) -> Option<R> {
        let input = self.input.read(cx);
        let diff = input.code.as_ref()?.diff.as_ref()?;
        Some(read(&diff.rows))
    }

    /// The selection as offsets of the side's text: (anchor, head).
    pub fn selection(&self, cx: &App) -> (usize, usize) {
        let selection = self.input.read(cx).model.selection();
        (selection.anchor, selection.head)
    }

    /// Selects `range` of the side's text and scrolls as `reveal` says.
    pub fn select(&self, range: Range<usize>, reveal: Reveal, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| {
            input.select_and_reveal(range, reveal, window, cx)
        });
    }

    pub fn scroll(&self, cx: &App) -> Point<Pixels> {
        self.input.read(cx).scroll
    }

    pub fn set_scroll(&self, scroll: Point<Pixels>, cx: &mut App) {
        self.input.update(cx, |input, cx| {
            input.scroll = scroll;
            input.autoscroll = false;
            cx.notify();
        });
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = self.focus_handle(cx);
        window.focus(&handle, cx);
    }

    /// The items of Go to symbol, from the parse of the working tree's file, in file order.
    pub fn outline_rows(&self, cx: &App) -> Vec<OutlineRow> {
        let input = self.input.read(cx);
        let (Some(lang), Some(source)) = (
            input.code.as_ref().and_then(|code| code.lang),
            input
                .code
                .as_ref()
                .and_then(|code| code.diff.as_ref())
                .and_then(|diff| diff.sources[1].as_ref()),
        ) else {
            return Vec::new();
        };
        let Some(tree) = source.syntax.as_ref().and_then(Syntax::tree) else {
            return Vec::new();
        };
        let text = &*source.text;
        outline(lang, tree, text)
            .iter()
            .map(|item| {
                let start = item.range.start;
                let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
                OutlineRow {
                    text: item.text.clone(),
                    depth: item.depth,
                    row: text[..start].matches('\n').count() as u32,
                    column: (start - line_start) as u32,
                }
            })
            .collect()
    }

    /// For the mouse tests: the side's left edge and the middle of shown row `row` in window
    /// pixels, and the x of char `column` of that row from the start of the text.
    #[cfg(test)]
    pub fn row_point(
        &self,
        row: usize,
        column: usize,
        window: &Window,
        cx: &mut App,
    ) -> Option<(Pixels, Pixels, Pixels)> {
        self.input.update(cx, |input, cx| {
            let bounds = input.last_bounds?;
            let laid = input.layout(window, cx);
            let at = laid.row_range(row).start + column;
            let x = laid.position(input.to_display(at), false).x;
            let y = bounds.top() + laid.line_height * (row as f32 + 0.5) - input.scroll.y;
            Some((bounds.left(), y, x))
        })
    }

    /// Where the rows were last laid out; None before the first draw.
    pub fn place(&self, window: &Window, cx: &mut App) -> Option<Place> {
        self.input.update(cx, |input, cx| {
            let bounds = input.last_bounds?;
            let laid = input.layout(window, cx);
            Some(Place {
                laid,
                scroll: input.scroll,
                origin: bounds.origin,
                size: bounds.size,
                room: input.bar_room(cx),
            })
        })
    }
}

/// The two files of a diff with their parse: drawn at once; a parse that runs out of time ends
/// on a background thread and draws the side again (A5).
fn parse_sources(
    texts: [&str; 2],
    lang: Option<&'static Lang>,
    cx: &mut Context<TextInput>,
) -> ([Option<Source>; 2], Vec<Task<()>>) {
    let mut parses = Vec::new();
    let sources = [0, 1].map(|index| {
        let text: Arc<str> = Arc::from(texts[index]);
        let mut syntax = lang.and_then(|lang| Syntax::new(lang).ok());
        if let Some(syntax) = syntax.as_mut()
            && !syntax.parse(&text, PARSE_BUDGET)
        {
            let job = syntax.job(text.clone());
            parses.push(cx.spawn(async move |input, cx| {
                let parsed = cx.background_spawn(async move { job.run() }).await;
                input
                    .update(cx, |input, cx| {
                        if let Some(code) = input.code.as_mut()
                            && let Some(diff) = code.diff.as_mut()
                            && let Some(Some(source)) = diff.sources.get_mut(index)
                            && let Some(syntax) = source.syntax.as_mut()
                            && syntax.finish(parsed)
                        {
                            code.colors += 1;
                            input.refresh_brackets();
                            cx.notify();
                        }
                    })
                    .ok();
            }));
        }
        Some(Source {
            lines: line_ranges(&text),
            text,
            syntax,
        })
    });
    (sources, parses)
}

impl FindTarget for DiffSide {
    fn id(&self) -> EntityId {
        self.input.entity_id()
    }

    fn text(&self, cx: &App) -> String {
        FindTarget::text(&self.code_view(), cx)
    }

    fn one_line_selection(&self, cx: &mut App) -> Option<String> {
        FindTarget::one_line_selection(&self.code_view(), cx)
    }

    fn caret(&self, cx: &mut App) -> usize {
        FindTarget::caret(&self.code_view(), cx)
    }

    fn show_matches(&self, ranges: &[Range<usize>], active: Option<usize>, cx: &mut App) {
        FindTarget::show_matches(&self.code_view(), ranges, active, cx)
    }

    fn clear_matches(&self, cx: &mut App) {
        FindTarget::clear_matches(&self.code_view(), cx)
    }

    fn select(&self, range: Range<usize>, window: &mut Window, cx: &mut App) {
        FindTarget::select(&self.code_view(), range, window, cx)
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        DiffSide::focus_handle(self, cx)
    }

    fn on_edited(&self, changed: Box<dyn Fn(&mut App)>, cx: &mut App) -> Subscription {
        FindTarget::on_edited(&self.code_view(), changed, cx)
    }
}

impl DiffSide {
    /// The same input as a code view, for what both share.
    fn code_view(&self) -> CodeView {
        CodeView::of_input(self.input.clone())
    }
}

/// Annotate with Git Blame on a diff side: rows are rows of the side's own file. A line that
/// does not show (collapsed, or a deleted line of an inline diff) has no label.
impl BlameTarget for DiffSide {
    fn lines(&self, cx: &App) -> u32 {
        let input = self.input.read(cx);
        input
            .code
            .as_ref()
            .and_then(|code| code.diff.as_ref())
            .and_then(DiffDecor::own_source)
            .map_or(0, |source| source.lines.len() as u32)
    }

    fn set_menu(&self, menu: RowMenu, cx: &mut App) {
        self.input.update(cx, |input, _| {
            if let Some(code) = input.code.as_mut() {
                code.menu = Some(menu);
            }
        });
    }

    fn set_labels(&self, labels: Vec<(u32, String)>, cx: &mut App) {
        self.input.update(cx, |input, cx| {
            let text = input.model.text();
            if let Some(code) = input.code.as_mut() {
                let shown = match code.diff.as_ref() {
                    Some(diff) => labels
                        .into_iter()
                        .filter_map(|(row, label)| {
                            let index = diff.rows.line_of(!diff.new_side, row)?;
                            Some((index as u32, label))
                        })
                        .collect(),
                    None => Vec::new(),
                };
                code.labels.set(shown, text);
                // The shown text changed with the text the same: lay it out again.
                code.colors += 1;
                input.model.track_edits();
            }
            cx.notify();
        });
    }

    fn clear_labels(&self, cx: &mut App) {
        self.set_labels(Vec::new(), cx);
    }
}

#[cfg(test)]
#[path = "diff_side_tests.rs"]
mod tests;
