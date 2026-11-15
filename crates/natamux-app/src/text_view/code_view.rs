//! The code view (plan 6.4, S4.3b): the own text input in full mode, plus what code needs:
//! - syntax colours from the own highlighter, parsed on the main thread within a budget and
//!   finished on a background thread when that runs out;
//! - indent guides and the active guide;
//! - the whitespace marks of the whole text while the diff's Show whitespace is on (that switch
//!   was Zed's `show_whitespaces` for every editor);
//! - the matching bracket and the other places of the selected text;
//! - the editor's scrollbar with its markers, one page of scroll past the end and a scroll margin
//!   of 3 lines (keys and edits only: a click does not scroll);
//! - the typing facts of its language (`code_rules.rs`).
//!
//! The JSON tab and the Docs views use it; the editor tab follows in S4.3c.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, Hsla, Pixels, Task, TextRun, Window, point, px,
};
use theme::ActiveTheme;
use tree_sitter::{InputEdit, Tree};

use super::code_rules::{Typing, find_all, typing_for};
use super::element::Layout;
use super::highlight::{Highlights, PARSE_BUDGET, Syntax};
use super::text_box::TextBox;
use super::text_input::{Mode, TAB_SIZE, TextInput};
use crate::app::appearance::Appearance;
use crate::git::languages::{Lang, lang_for};
use crate::text::model::{EditKind, TextModel};
use crate::text::motion;

/// The scroll margin of the editor, in lines.
const SCROLL_MARGIN: f32 = 3.;
/// The width of the editor's scrollbar.
pub const BAR_WIDTH: Pixels = px(15.);
/// The shortest thumb.
const MIN_THUMB: Pixels = px(25.);
/// The height of a cursor marker on the scrollbar.
const CURSOR_MARKER: Pixels = px(2.);
/// The shortest marker of a selected-text place.
const MIN_MARKER: Pixels = px(5.);
/// How long the scrollbar stays after a scroll.
const BAR_SHOWS_FOR: Duration = Duration::from_secs(1);
/// The wait before the other places of the selected text show.
const OCCURRENCE_DELAY: Duration = Duration::from_millis(100);
/// The wheel scrolls this much faster with Alt held.
pub const FAST_SCROLL: f32 = 4.;

/// A code view: build it, then hand its `TextBox` to the form parts and the owner.
#[derive(Clone)]
pub struct CodeView {
    input: Entity<TextInput>,
}

impl CodeView {
    /// `text` in the language of `path`, with indent guides, the caret at the start and nothing
    /// to undo, as a new Zed editor had it.
    pub fn new(
        path: &str,
        text: &str,
        current_line: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let lang = lang_for(path);
        let input = cx.new(|cx| {
            let mut input = TextInput::full(current_line, false, window, cx);
            input.model = TextModel::with_text(text);
            input.code = Some(Code::new(lang, true));
            input.sync_code(cx);
            input
        });
        Self { input }
    }

    /// Plain text with the line of the caret painted: the Docs edit box and the Docs source
    /// (`read_only`). No guides, as in the old Docs editors.
    pub fn plain(read_only: bool, window: &mut Window, cx: &mut App) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::full(true, read_only, window, cx);
            input.code = Some(Code::new(None, false));
            input
        });
        Self { input }
    }

    pub fn text_box(&self) -> TextBox {
        TextBox::of_input(&self.input)
    }

    /// The name of the language the text is coloured as.
    #[cfg(test)]
    pub fn language_name(&self, cx: &App) -> Option<&'static str> {
        let code = self.input.read(cx).code.as_ref()?;
        code.lang.map(|lang| lang.name)
    }

    #[cfg(test)]
    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    /// How many colours the runs of the text have: more than one once it is parsed.
    #[cfg(test)]
    pub fn run_colors(&self, cx: &App) -> usize {
        let input = self.input.read(cx);
        let base = TextRun {
            len: 0,
            font: gpui::font(".ZedMono"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = input
            .code_runs(&base, input.model.text().len(), cx)
            .unwrap_or_default();
        let mut colors: Vec<Hsla> = Vec::new();
        for run in runs {
            if !colors.contains(&run.color) {
                colors.push(run.color);
            }
        }
        colors.len()
    }
}

/// An auto-closed pair: the offsets of its opening and closing chars. Typing the closing char
/// at it steps over it, and Backspace right after the opening char deletes both.
#[derive(Clone, Copy, Debug)]
struct Region {
    open: usize,
    close: usize,
}

/// The state of a code view's scrollbar.
#[derive(Default)]
pub(super) struct Bar {
    scrolled_at: Option<Instant>,
    hovered: bool,
    /// While the thumb is dragged: where it was taken, from its top.
    grab: Option<Pixels>,
    _hide: Option<Task<()>>,
}

/// Where the scrollbar is drawn.
pub(super) struct BarBounds {
    pub track: Bounds<Pixels>,
    pub thumb: Bounds<Pixels>,
}

/// What a text input has more as a code view.
pub struct Code {
    pub(super) lang: Option<&'static Lang>,
    pub(super) guides: bool,
    syntax: Option<Syntax>,
    /// The text the tree was last moved to.
    parsed: Arc<str>,
    /// Goes up when the colours change with the text the same (a parse landed).
    pub(super) colors: u64,
    _parse: Option<Task<()>>,
    regions: Vec<Region>,
    /// The text version the regions belong to: any other edit drops them.
    regions_version: u64,
    pub(super) bar: Bar,
    /// The places of the selected text (the selection among them), the selected text and the
    /// text version they are of.
    pub(super) occurrences: Vec<Range<usize>>,
    occurrences_of: Option<String>,
    occurrences_version: u64,
    _occurrences: Option<Task<()>>,
    /// The matching brackets, found at the last change of the selection, and the text version
    /// they are of.
    brackets: Vec<Range<usize>>,
    brackets_version: u64,
}

impl Code {
    fn new(lang: Option<&'static Lang>, guides: bool) -> Self {
        Code {
            lang,
            guides,
            syntax: lang.and_then(|lang| Syntax::new(lang).ok()),
            parsed: Arc::from(""),
            colors: 0,
            _parse: None,
            regions: Vec::new(),
            regions_version: 0,
            bar: Bar::default(),
            occurrences: Vec::new(),
            occurrences_of: None,
            occurrences_version: 0,
            _occurrences: None,
            brackets: Vec::new(),
            brackets_version: 0,
        }
    }

    pub(super) fn tree(&self) -> Option<&Tree> {
        self.syntax.as_ref()?.tree()
    }

    fn typing(&self) -> Option<&'static Typing> {
        typing_for(self.lang?.name)
    }

    /// The places of the selected text other than `selected` itself.
    pub(super) fn other_occurrences(
        &self,
        selected: Range<usize>,
    ) -> impl Iterator<Item = &Range<usize>> {
        self.occurrences
            .iter()
            .filter(move |range| **range != selected)
    }
}

/// The edit that turns `old` into `new`, as one change between their common start and end.
fn input_edit(old: &str, new: &str) -> InputEdit {
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    InputEdit {
        start_byte: prefix,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: ts_point(old, prefix),
        old_end_position: ts_point(old, old_end),
        new_end_position: ts_point(new, new_end),
    }
}

fn ts_point(text: &str, at: usize) -> tree_sitter::Point {
    let before = &text.as_bytes()[..at];
    let row = before.iter().filter(|b| **b == b'\n').count();
    let line = before
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |at| at + 1);
    tree_sitter::Point::new(row, at - line)
}

fn parse(lang: &Lang, text: &str) -> Option<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).ok()?;
    parser.parse(text, None)
}

/// `at` moved down to a char boundary of `text`.
fn boundary(text: &str, at: usize) -> usize {
    motion::floor_boundary(text, at)
}

impl TextInput {
    /// The typing facts of the code view's language, if it has any.
    pub(super) fn typing(&self) -> Option<&'static Typing> {
        self.code.as_ref()?.typing()
    }

    /// The chars that count as word chars next to letters, digits and `_`.
    pub(super) fn word_chars(&self) -> &'static str {
        self.typing().map_or("", |typing| typing.word_chars)
    }

    /// After any change of the text or the selection: the tree follows the text, and the other
    /// places of the selected text show after a short wait, searched again after each edit as
    /// Zed's editor does.
    pub(super) fn sync_code(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.code.as_mut() else {
            return;
        };
        let text = self.model.text();
        if let Some(syntax) = code.syntax.as_mut()
            && (*code.parsed != *text || syntax.tree().is_none())
        {
            syntax.edit(&input_edit(&code.parsed, text));
            code.parsed = Arc::from(text);
            code.colors += 1;
            if !syntax.parse(text, PARSE_BUDGET) {
                let job = syntax.job(code.parsed.clone());
                code._parse = Some(cx.spawn(async move |input, cx| {
                    let parsed = cx.background_spawn(async move { job.run() }).await;
                    input
                        .update(cx, |input, cx| {
                            if let Some(code) = input.code.as_mut()
                                && let Some(syntax) = code.syntax.as_mut()
                                && syntax.finish(parsed)
                            {
                                code.colors += 1;
                                cx.notify();
                            }
                        })
                        .ok();
                }));
            }
        }
        let selection = self.model.selection().range();
        let selected = &text[selection];
        let query =
            (!selected.trim().is_empty() && !selected.contains('\n')).then(|| selected.to_owned());
        let version = self.model.version();
        if query != code.occurrences_of || version != code.occurrences_version {
            code.occurrences.clear();
            code.occurrences_of = query.clone();
            code.occurrences_version = version;
            code._occurrences = query.map(|query| {
                cx.spawn(async move |input, cx| {
                    cx.background_executor().timer(OCCURRENCE_DELAY).await;
                    input
                        .update(cx, |input, cx| {
                            let found = find_all(input.model.text(), &query);
                            let version = input.model.version();
                            if let Some(code) = input.code.as_mut()
                                && code.occurrences_of.as_deref() == Some(query.as_str())
                                && code.occurrences_version == version
                            {
                                code.occurrences = found;
                                cx.notify();
                            }
                        })
                        .ok();
                })
            });
        }
    }

    /// The runs of the coloured text, from `base`; None when the text has no colours (no
    /// language, no tree yet, or the shown text is not the text).
    pub(super) fn code_runs(&self, base: &TextRun, len: usize, cx: &App) -> Option<Vec<TextRun>> {
        let code = self.code.as_ref()?;
        let syntax = code.syntax.as_ref()?;
        let tree = syntax.tree()?;
        let text = self.model.text();
        if text.len() != len {
            return None;
        }
        let highlights = Highlights::of(syntax.lang()).ok()?;
        let theme = cx.theme().syntax();
        let keys = highlights.theme_keys(|key| theme.style_for_name(key).is_some());
        let mut runs = Vec::new();
        let mut at = 0;
        for (range, key) in highlights.runs(tree, text, 0..len, &keys) {
            // An older tree, moved by the edits, may cut into a char.
            let start = boundary(text, range.start.min(len)).max(at);
            let end = boundary(text, range.end.min(len));
            if start >= end {
                continue;
            }
            if start > at {
                runs.push(TextRun {
                    len: start - at,
                    ..base.clone()
                });
            }
            let style = theme.style_for_name(&key).unwrap_or_default();
            let mut font = base.font.clone();
            if let Some(weight) = style.font_weight {
                font.weight = weight;
            }
            if let Some(slant) = style.font_style {
                font.style = slant;
            }
            runs.push(TextRun {
                len: end - start,
                font,
                color: style.color.unwrap_or(base.color),
                ..base.clone()
            });
            at = end;
        }
        if at < len {
            runs.push(TextRun {
                len: len - at,
                ..base.clone()
            });
        }
        Some(runs)
    }

    /// The auto-closed pairs that still hold the caret, taken out of the view: none after an
    /// edit that was not typed through them.
    fn take_regions(&mut self) -> Vec<Region> {
        let (version, head) = (self.model.version(), self.model.head());
        let Some(code) = self.code.as_mut() else {
            return Vec::new();
        };
        let mut regions = std::mem::take(&mut code.regions);
        if code.regions_version != version {
            regions.clear();
        }
        regions.retain(|region| region.open < head && head <= region.close);
        regions
    }

    /// Keeps `regions` for the text as it is now.
    fn put_regions(&mut self, regions: Vec<Region>) {
        let version = self.model.version();
        if let Some(code) = self.code.as_mut() {
            code.regions = regions;
            code.regions_version = version;
        }
    }

    /// One char typed at the selection of a code view with typing facts: a pair closes itself,
    /// a selection gets surrounded, and the closing char of an auto-closed pair is stepped over.
    /// False when the view leaves the text to the plain input.
    pub(super) fn type_code(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(typing) = self.typing() else {
            return false;
        };
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return false;
        };
        let selection = self.model.selection();
        let now = self.now(cx);
        if !selection.is_empty() {
            let Some(close) = typing.surround(c) else {
                return false;
            };
            let range = selection.range();
            let inner = self.model.text()[range.clone()].to_owned();
            let wrapped = format!("{c}{inner}{close}");
            let edited = self
                .model
                .edit(range.clone(), &wrapped, EditKind::Typing, now);
            let start = range.start + c.len_utf8();
            self.model.select(start, start + inner.len());
            self.put_regions(Vec::new());
            self.changed(edited, cx);
            return true;
        }
        let at = selection.head;
        let over = self.model.text()[at..].starts_with(c);
        let mut regions = self.take_regions();
        if over && let Some(index) = regions.iter().position(|region| region.close == at) {
            regions.remove(index);
            self.model.move_to(at + c.len_utf8());
            self.model.goal_x = None;
            self.put_regions(regions);
            self.changed(false, cx);
            return true;
        }
        let in_string = self
            .code
            .as_ref()
            .and_then(Code::tree)
            .is_some_and(|tree| typing.in_string(tree, at));
        let closer = typing.closer(self.model.text(), at, c, in_string);
        let insert = closer.map_or_else(|| c.to_string(), |close| format!("{c}{close}"));
        let edited = self.model.edit(at..at, &insert, EditKind::Typing, now);
        let after = at + c.len_utf8();
        self.model.move_to(after);
        for region in &mut regions {
            region.close += c.len_utf8();
        }
        if closer.is_some() {
            regions.push(Region {
                open: at,
                close: after,
            });
        }
        self.put_regions(regions);
        self.changed(edited, cx);
        true
    }

    /// Backspace right after the opening char of an auto-closed pair deletes both chars.
    pub(super) fn delete_pair(&mut self, cx: &mut Context<Self>) -> bool {
        if self.typing().is_none() || !self.model.selection().is_empty() || self.read_only {
            return false;
        }
        let at = self.model.head();
        let mut regions = self.take_regions();
        let Some(index) = regions
            .iter()
            .position(|region| region.close == at && region.open + 1 == at)
        else {
            self.put_regions(regions);
            return false;
        };
        let region = regions.remove(index);
        let now = self.now(cx);
        let edited = self
            .model
            .edit(region.open..region.close + 1, "", EditKind::Deleting, now);
        self.changed(edited, cx);
        true
    }

    /// Enter in a code view with typing facts: between the chars of a pair that opens a block,
    /// the caret goes on an indented line of its own and the closing char on the next; else the
    /// new line takes the indent the language gives it.
    pub(super) fn newline_code(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(typing), Some(lang)) = (self.typing(), self.code.as_ref().and_then(|c| c.lang))
        else {
            return false;
        };
        let range = self.model.selection().range();
        let text = self.model.text();
        let line_start = motion::line_start(text, range.start);
        let lead: String = text[line_start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let (insert, caret) = if range.is_empty() && typing.expands(text, range.start) {
            let middle = format!("\n{lead}{}", " ".repeat(TAB_SIZE));
            let caret = range.start + middle.len();
            (format!("{middle}\n{lead}"), caret)
        } else {
            let mut after = text.to_owned();
            after.replace_range(range.clone(), "\n");
            let row = after[..=range.start].matches('\n').count();
            let indent =
                parse(lang, &after).map_or(lead, |tree| typing.indent_for(&after, &tree, row));
            let insert = format!("\n{indent}");
            let caret = range.start + insert.len();
            (insert, caret)
        };
        let now = self.now(cx);
        let edited = self.model.edit(range, &insert, EditKind::Typing, now);
        self.model.move_to(caret);
        self.put_regions(Vec::new());
        self.changed(edited, cx);
        true
    }

    /// The most a code view scrolls: until its last row is at the top.
    pub(super) fn code_scroll_limit(&self, laid: &Layout) -> Option<Pixels> {
        self.code.as_ref()?;
        Some(laid.line_height * laid.rows.saturating_sub(1) as f32)
    }

    /// The width the scrollbar takes from the wrap width of a code view: Zed's editor keeps room
    /// for it unless its scrollbars never show (the shots).
    pub(super) fn bar_room(&self, cx: &App) -> Pixels {
        let never = cx
            .try_global::<Appearance>()
            .is_some_and(|look| look.hide_scrollbars);
        if self.code.is_some() && matches!(self.mode, Mode::Full { .. }) && !never {
            BAR_WIDTH
        } else {
            px(0.)
        }
    }

    /// The scroll margin of a code view at `height`: 3 lines, less when the view is short.
    pub(super) fn code_margin(&self, laid: &Layout, height: Pixels) -> Pixels {
        if self.code.is_none() {
            return px(0.);
        }
        let shown = (height / laid.line_height).floor();
        laid.line_height * SCROLL_MARGIN.min(((shown - 1.) / 2.).floor()).max(0.)
    }

    /// A wheel or bar scroll of a code view: the scrollbar shows and hides again 1 s later.
    pub(super) fn code_scrolled(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(code) = self.code.as_mut() else {
            return;
        };
        code.bar.scrolled_at = Some(now);
        code.bar._hide = Some(cx.spawn(async move |input, cx| {
            cx.background_executor().timer(BAR_SHOWS_FOR).await;
            input.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    /// Where the scrollbar of a code view is, when it shows: always in a `-scrollbar` shot, never
    /// in other shots, else while it is dragged, for 1 s after a scroll, while the caret is out
    /// of view, and while the selected text has other places.
    pub(super) fn bar_bounds(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        cx: &App,
    ) -> Option<BarBounds> {
        let code = self.code.as_ref()?;
        if !matches!(self.mode, Mode::Full { .. }) {
            return None;
        }
        let look = cx.try_global::<Appearance>();
        if look.is_some_and(|look| look.hide_scrollbars) {
            return None;
        }
        let height = bounds.size.height;
        let caret = laid
            .position(self.to_display(self.model.head()), self.end_bias)
            .y;
        let caret_hidden =
            caret + laid.line_height <= self.scroll.y || caret >= self.scroll.y + height;
        let recent = code
            .bar
            .scrolled_at
            .is_some_and(|at| self.now(cx).saturating_duration_since(at) < BAR_SHOWS_FOR);
        let always = look.is_some_and(|look| look.always_scrollbars);
        let occurrences = code
            .other_occurrences(self.model.selection().range())
            .next()
            .is_some();
        if !(always || recent || caret_hidden || occurrences || code.bar.grab.is_some()) {
            return None;
        }
        let limit = self.code_scroll_limit(laid).unwrap_or_default();
        let content = limit + height;
        let thumb_height = (height * (height / content)).max(MIN_THUMB).min(height);
        let top = if limit > px(0.) {
            (height - thumb_height) * (self.scroll.y / limit)
        } else {
            px(0.)
        };
        let track = Bounds::new(
            point(bounds.right() - BAR_WIDTH, bounds.top()),
            gpui::size(BAR_WIDTH, height),
        );
        let thumb = Bounds::new(
            point(track.left(), track.top() + top),
            gpui::size(BAR_WIDTH, thumb_height),
        );
        Some(BarBounds { track, thumb })
    }

    pub(super) fn bar_hovered(&self) -> bool {
        self.code.as_ref().is_some_and(|code| code.bar.hovered)
    }

    pub(super) fn bar_dragged(&self) -> bool {
        self.code
            .as_ref()
            .is_some_and(|code| code.bar.grab.is_some())
    }

    /// A press on the scrollbar: on the thumb it takes it, on the track the thumb jumps there
    /// first. True when the press was on the bar.
    pub(super) fn bar_press(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bounds) = self.last_bounds.filter(|_| self.code.is_some()) else {
            return false;
        };
        let laid = self.layout(window, cx);
        let Some(bar) = self.bar_bounds(&laid, bounds, cx) else {
            return false;
        };
        if !bar.track.contains(&position) {
            return false;
        }
        let grab = if bar.thumb.contains(&position) {
            position.y - bar.thumb.top()
        } else {
            bar.thumb.size.height / 2.
        };
        if let Some(code) = self.code.as_mut() {
            code.bar.grab = Some(grab);
        }
        self.bar_drag(position, window, cx);
        true
    }

    /// Moves a taken thumb with the pointer, and keeps the hover state. True while the thumb is
    /// taken.
    pub(super) fn bar_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bounds) = self.last_bounds.filter(|_| self.code.is_some()) else {
            return false;
        };
        let laid = self.layout(window, cx);
        let bar = self.bar_bounds(&laid, bounds, cx);
        let hovered = bar
            .as_ref()
            .is_some_and(|bar| bar.thumb.contains(&position));
        let Some(code) = self.code.as_mut() else {
            return false;
        };
        if code.bar.hovered != hovered {
            code.bar.hovered = hovered;
            cx.notify();
        }
        let (Some(grab), Some(bar)) = (code.bar.grab, bar) else {
            return false;
        };
        let limit = self.code_scroll_limit(&laid).unwrap_or_default();
        let room = bar.track.size.height - bar.thumb.size.height;
        if room > px(0.) {
            let top = (position.y - grab - bar.track.top()).clamp(px(0.), room);
            self.scroll.y = limit * (top / room);
        }
        self.clamp_scroll(&laid, bounds);
        self.code_scrolled(cx);
        cx.notify();
        true
    }

    /// Lets go of the thumb. True when it was taken.
    pub(super) fn bar_release(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(code) = self.code.as_mut() else {
            return false;
        };
        let taken = code.bar.grab.take().is_some();
        if taken {
            cx.notify();
        }
        taken
    }

    /// The smallest bracket pair around an empty caret, its edges included as in Zed's editor
    /// (`|{}` and `{}|` count): the ranges of its two chars.
    pub(super) fn matching_brackets(&self) -> Vec<Range<usize>> {
        let (Some(typing), Some(tree)) = (self.typing(), self.code.as_ref().and_then(Code::tree))
        else {
            return Vec::new();
        };
        let selection = self.model.selection();
        if !selection.is_empty() {
            return Vec::new();
        }
        let (at, text) = (selection.head, self.model.text());
        let mut best: Option<(Range<usize>, Range<usize>)> = None;
        // From the node before the caret too: right after `}` the caret is outside its node.
        for from in [at.saturating_sub(1), at] {
            let mut node = tree.root_node().descendant_for_byte_range(from, from);
            while let Some(found) = node {
                node = found.parent();
                let count = found.child_count();
                let (Some(first), Some(last)) = (found.child(0), found.child(count.max(1) - 1))
                else {
                    continue;
                };
                let (open, close) = (first.byte_range(), last.byte_range());
                let pair = count >= 2
                    && !last.is_missing()
                    && open.len() == 1
                    && close.len() == 1
                    && open.start <= at
                    && at <= close.end
                    && typing.pairs.iter().any(|pair| {
                        text[open.clone()].starts_with(pair.open)
                            && text[close.clone()].starts_with(pair.close)
                    });
                let shorter = best
                    .as_ref()
                    .is_none_or(|(o, c)| close.end - open.start < c.end - o.start);
                if pair && shorter {
                    best = Some((open, close));
                }
            }
        }
        best.map_or_else(Vec::new, |(open, close)| vec![open, close])
    }

    /// After a change of the selection: the matching brackets of the new caret. They stay after
    /// a blur, as in Zed's editor, and go when the text changes without the selection.
    pub(super) fn refresh_brackets(&mut self) {
        if self.code.is_none() {
            return;
        }
        let found = self.matching_brackets();
        let version = self.model.version();
        if let Some(code) = self.code.as_mut() {
            code.brackets = found;
            code.brackets_version = version;
        }
    }

    /// The markers on the scrollbar `bar`, as Zed's editor paints them: each other place of the
    /// selected text in the middle third, then the caret over the full width.
    fn bar_markers(
        &self,
        laid: &Layout,
        bar: &BarBounds,
        cx: &App,
    ) -> Vec<(Bounds<Pixels>, Hsla)> {
        let Some(code) = self.code.as_ref() else {
            return Vec::new();
        };
        let line_height = laid.line_height;
        let scroll_rows = laid.rows.saturating_sub(1) as f32;
        let unit = if scroll_rows > 0. {
            (bar.track.size.height - bar.thumb.size.height) / scroll_rows
        } else {
            line_height
        };
        let row = |at: usize| (laid.position(self.to_display(at), false).y / line_height).round();
        let border = px(1.);
        let column = ((BAR_WIDTH - border) / 3.).floor();
        let (left, top) = (bar.track.left(), bar.track.top());
        let mut markers: Vec<(Bounds<Pixels>, Hsla)> = Vec::new();
        let info = cx.theme().status().info;
        let selected = self.model.selection().range();
        for range in code.other_occurrences(selected) {
            let start = top + unit * row(range.start);
            let end = top + unit * row(range.end) + unit.max(MIN_MARKER);
            // Touching places merge into one marker.
            if let Some((last, _)) = markers.last_mut()
                && last.bottom() >= start - px(1.)
            {
                last.size.height = end.max(last.bottom()) - last.top();
                continue;
            }
            markers.push((
                Bounds::from_corners(
                    point(left + border + column, start),
                    point(left + border + column * 2., end),
                ),
                info,
            ));
        }
        let head = top + unit * row(self.model.head());
        markers.push((
            Bounds::from_corners(
                point(left + border, head),
                point(left + BAR_WIDTH, head + CURSOR_MARKER),
            ),
            cx.theme().players().local().cursor,
        ));
        markers
    }
}

/// What a code view paints past the plain input, in window coordinates.
#[derive(Default)]
pub(super) struct CodePaint {
    /// Each indent guide line, and whether it is active.
    pub guides: Vec<(Bounds<Pixels>, bool)>,
    /// The matching brackets and the other places of the selected text.
    pub backgrounds: Vec<Bounds<Pixels>>,
    /// Whitespace marks for the whole text, not only inside the selection.
    pub all_whitespace: bool,
    /// The scrollbar, hovered, dragged.
    pub bar: Option<(BarBounds, bool, bool)>,
    /// The markers on the scrollbar, painted before its thumb.
    pub markers: Vec<(Bounds<Pixels>, Hsla)>,
}

impl TextInput {
    /// The parts of a code view to paint over `bounds`, where the text starts at `origin`.
    pub(super) fn code_paint(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        origin: gpui::Point<Pixels>,
        cx: &App,
    ) -> CodePaint {
        let Some(code) = self.code.as_ref() else {
            return CodePaint::default();
        };
        let text = self.model.text();
        let line_height = laid.line_height;
        let mut paint = CodePaint {
            all_whitespace: cx
                .try_global::<Appearance>()
                .is_some_and(|look| look.show_whitespace),
            bar: self
                .bar_bounds(laid, bounds, cx)
                .map(|bar| (bar, self.bar_hovered(), self.bar_dragged())),
            ..CodePaint::default()
        };
        if let Some((bar, _, _)) = &paint.bar {
            paint.markers = self.bar_markers(laid, bar, cx);
        }
        if code.guides {
            let found = super::code_rules::guides(text);
            let caret_row = text[..self.model.head()].matches('\n').count();
            let active = super::code_rules::active_guides(text, &found, caret_row);
            for (index, guide) in found.iter().enumerate() {
                let (Some(first), Some(last)) =
                    (laid.lines.get(guide.start), laid.lines.get(guide.end))
                else {
                    continue;
                };
                let top = origin.y + line_height * first.first_row as f32;
                let rows = last.first_row + last.row_starts.len() - first.first_row;
                let height = line_height * rows as f32;
                if top + height < bounds.top() || top > bounds.bottom() {
                    continue;
                }
                let x = origin.x + laid.em * (guide.depth * TAB_SIZE) as f32;
                paint.guides.push((
                    Bounds::new(point(x, top), gpui::size(px(1.), height)),
                    active.contains(&index),
                ));
            }
        }
        let version = self.model.version();
        let mut ranges: Vec<Range<usize>> = if code.brackets_version == version {
            code.brackets.clone()
        } else {
            Vec::new()
        };
        // The selected text itself shows only in the selection colour.
        ranges.extend(code.other_occurrences(self.model.selection().range()).cloned());
        for range in ranges {
            for rect in laid.selection_rects(range) {
                let rect = Bounds::new(rect.origin + origin, rect.size);
                if rect.bottom() >= bounds.top() && rect.top() <= bounds.bottom() {
                    paint.backgrounds.push(rect);
                }
            }
        }
        paint
    }
}
