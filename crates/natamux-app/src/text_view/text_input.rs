//! The own text input (plan 6.3, S3.3): one line, an area that grows with its text, or a full
//! text view. The text lives in `crate::text::model`; this view takes the keys of
//! `app/text_boxes.rs` as its own actions, the mouse and the input method (`ime.rs`), and
//! `element.rs` draws it. It tells its owner only by `TextEvent`s, as effects (6.3.8).

use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AbsoluteLength, App, Bounds, ClipboardItem, Context, CursorStyle, EventEmitter, FocusHandle,
    Focusable, Font, KeyContext, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Render, ScrollWheelEvent, SharedString, Subscription, TextRun, TextStyle,
    TextStyleRefinement, UnderlineStyle, Window, div, font, point, px, relative, rems,
};
use theme::ActiveTheme;

use super::blink::Blink;
use super::element::{CARET_WIDTH, Layout, LayoutKey, TextElement};
use super::text_box::TextEvent;
use crate::app::appearance::Appearance;
use crate::components::form::input_style;
use crate::text::model::{EditKind, Selection, TextModel};
use crate::text::motion;

/// The own keys of a text input. They carry the names of Zed's editor actions, in their own
/// namespace, so both can be bound to the same key (6.3.1).
pub mod actions {
    gpui::actions!(
        text_input,
        [
            Backspace,
            Delete,
            MoveLeft,
            MoveRight,
            SelectLeft,
            SelectRight,
            MoveToBeginningOfLine,
            MoveToEndOfLine,
            SelectToBeginningOfLine,
            SelectToEndOfLine,
            MoveToPreviousWordStart,
            MoveToNextWordEnd,
            SelectToPreviousWordStart,
            SelectToNextWordEnd,
            DeleteToPreviousWordStart,
            DeleteToNextWordEnd,
            DeleteToBeginningOfLine,
            SelectAll,
            Copy,
            Cut,
            Paste,
            Undo,
            Redo,
            Newline,
            MoveUp,
            MoveDown,
            SelectUp,
            SelectDown,
            MovePageUp,
            MovePageDown,
            MoveToBeginning,
            MoveToEnd,
            SelectToBeginning,
            SelectToEnd,
            Tab,
            Backtab,
        ]
    );
}

/// The kinds of box. The key context names them as Zed's editor did, so the bindings of
/// `text_boxes.rs` and `commands_editor.rs` still match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    OneLine,
    /// Grows from `min` to `max` lines with its text, then scrolls inside.
    Area { min: usize, max: usize },
    /// Fills its box and scrolls inside; `current_line` paints the caret's row.
    Full { current_line: bool },
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::OneLine => "single_line",
            Mode::Area { .. } => "auto_height",
            Mode::Full { .. } => "full",
        }
    }
}

/// The text size of one-line boxes and areas, in rem (14 px at UI size 16).
const INPUT_FONT_SIZE: f32 = 0.875;
/// Zed's "comfortable" line height, before the box's own style.
const LINE_HEIGHT: f32 = 1.618;
/// A press inside the selection held this long, then moved, drags the text (D36).
const DRAG_DELAY: Duration = Duration::from_millis(300);
const TAB_SIZE: usize = 4;

/// A left press that is still down.
struct Press {
    at: usize,
    time: Instant,
    clicks: usize,
    /// It came down inside the selection, which stays until the release or a move.
    inside: bool,
    moved: bool,
    /// It holds the selected text and drops it at the release.
    moving_text: bool,
    /// The selection the press made (a word or a line for more clicks).
    origin: Selection,
}

pub struct TextInput {
    pub(super) focus: FocusHandle,
    pub(super) model: TextModel,
    pub(super) mode: Mode,
    placeholder: SharedString,
    masked: bool,
    pub(super) read_only: bool,
    style: TextStyleRefinement,
    pub(super) blink: Blink,
    /// How far the text is scrolled: x in one line, y in the others.
    pub(super) scroll: Point<Pixels>,
    /// The next frame scrolls the caret into view.
    pub(super) autoscroll: bool,
    /// A caret at a wrap point shows at the end of the row above (after End).
    pub(super) end_bias: bool,
    pub(super) last_bounds: Option<Bounds<Pixels>>,
    laid: Option<Rc<Layout>>,
    press: Option<Press>,
    /// Where held text would go in, while it is dragged.
    pub(super) drop_at: Option<usize>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TextEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TextInput {
    pub fn one_line(placeholder: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(Mode::OneLine, placeholder, input_style(None, None), window, cx)
    }

    pub fn area(
        placeholder: &str,
        min: usize,
        max: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = Mode::Area { min, max };
        Self::new(mode, placeholder, input_style(None, None), window, cx)
    }

    pub fn fill(placeholder: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mode = Mode::Full {
            current_line: false,
        };
        Self::new(mode, placeholder, input_style(None, None), window, cx)
    }

    pub fn plain(read_only: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mode = Mode::Full { current_line: true };
        let mut input = Self::new(mode, "", TextStyleRefinement::default(), window, cx);
        input.read_only = read_only;
        input
    }

    fn new(
        mode: Mode,
        placeholder: &str,
        style: TextStyleRefinement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus(&focus, window, |input, _, cx| {
                input.start_blink(cx);
                cx.notify();
            }),
            cx.on_blur(&focus, window, |input, _, cx| input.blur(cx)),
            cx.observe_global::<Appearance>(|_, cx| cx.notify()),
        ];
        TextInput {
            focus,
            model: TextModel::default(),
            mode,
            placeholder: SharedString::from(placeholder.to_string()),
            masked: false,
            read_only: false,
            style,
            blink: Blink::default(),
            scroll: Point::default(),
            autoscroll: false,
            end_bias: false,
            last_bounds: None,
            laid: None,
            press: None,
            drop_at: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn text(&self) -> &str {
        self.model.text()
    }

    pub fn is_composing(&self) -> bool {
        self.model.marked().is_some()
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// The box as it was last drawn, in window coordinates.
    pub fn last_bounds(&self) -> Option<&Bounds<Pixels>> {
        self.last_bounds.as_ref()
    }

    /// The placeholder, or None when there is none.
    pub fn placeholder_text(&self, _: &App) -> Option<String> {
        (!self.placeholder.is_empty()).then(|| self.placeholder.to_string())
    }

    pub fn set_placeholder(&mut self, text: &str, cx: &mut Context<Self>) {
        self.placeholder = SharedString::from(text.to_string());
        cx.notify();
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        self.masked = masked;
        cx.notify();
    }

    pub fn set_style(&mut self, style: TextStyleRefinement, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    pub(super) fn now(&self, cx: &App) -> Instant {
        cx.background_executor().now()
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let edited = self.model.set_text(text, now);
        self.changed(edited, cx);
    }

    pub fn replace_minimal(&mut self, text: &str, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let edited = self.model.replace_minimal(text, now);
        if edited {
            cx.emit(TextEvent::Edited);
        }
        // The caret and the scroll stay: no autoscroll.
        cx.notify();
    }

    pub fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        let range = self.model.selection().range();
        self.edit(range, text, EditKind::Other, cx);
    }

    pub fn select_everything(&mut self, cx: &mut Context<Self>) {
        let len = self.model.text().len();
        self.model.select(0, len);
        self.changed(false, cx);
    }

    pub fn caret_to_end(&mut self, cx: &mut Context<Self>) {
        let len = self.model.text().len();
        self.move_caret(len, false, cx);
    }

    /// After a change of the text or the caret: show the caret, keep it in view, and tell the
    /// owner when the text changed.
    pub(super) fn changed(&mut self, edited: bool, cx: &mut Context<Self>) {
        self.end_bias = false;
        self.autoscroll = true;
        self.pause_blink(cx);
        if edited {
            cx.emit(TextEvent::Edited);
        }
        cx.notify();
    }

    pub(super) fn edit(
        &mut self,
        range: Range<usize>,
        text: &str,
        kind: EditKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let now = self.now(cx);
        let edited = self.model.edit(range, text, kind, now);
        self.changed(edited, cx);
        edited
    }

    fn move_caret(&mut self, at: usize, select: bool, cx: &mut Context<Self>) {
        if select {
            self.model.select_to(at);
        } else {
            self.model.move_to(at);
        }
        self.model.goal_x = None;
        self.changed(false, cx);
    }

    fn blur(&mut self, cx: &mut Context<Self>) {
        // The word an input method was composing stays as typed.
        if self.model.marked().is_some() {
            self.model.set_marked(None);
            self.model.seal();
        }
        self.press = None;
        self.drop_at = None;
        self.stop_blink();
        cx.emit(TextEvent::Blurred);
        cx.notify();
    }

    /// The text style: the UI font at 0.875 rem for one line and areas, the buffer font for full
    /// boxes, then the box's own style.
    pub(super) fn text_style(&self, cx: &App) -> TextStyle {
        let appearance = cx.try_global::<Appearance>();
        let full = matches!(self.mode, Mode::Full { .. });
        let (base, size, line_height): (Font, AbsoluteLength, f32) = match (full, appearance) {
            (true, Some(look)) => (
                look.buffer_font.clone(),
                look.buffer_font_size.into(),
                look.buffer_line_height,
            ),
            (true, None) => (font(".ZedMono"), px(13.).into(), LINE_HEIGHT),
            (false, Some(look)) => (
                look.ui_font.clone(),
                rems(INPUT_FONT_SIZE).into(),
                LINE_HEIGHT,
            ),
            (false, None) => (font(".ZedSans"), rems(INPUT_FONT_SIZE).into(), LINE_HEIGHT),
        };
        let mut style = TextStyle {
            color: cx.theme().colors().editor_foreground,
            font_family: base.family,
            font_features: base.features,
            font_fallbacks: base.fallbacks,
            font_size: size,
            line_height: relative(line_height),
            font_weight: base.weight,
            font_style: base.style,
            ..TextStyle::default()
        };
        gpui::Refineable::refine(&mut style, &self.style);
        style
    }

    /// The text to draw, and whether it is the placeholder.
    pub(super) fn display(&self) -> (SharedString, bool) {
        let text = self.model.text();
        if text.is_empty() {
            (self.placeholder.clone(), true)
        } else if self.masked {
            (SharedString::from("*".repeat(text.chars().count())), false)
        } else {
            (SharedString::from(text.to_string()), false)
        }
    }

    /// One run of the text style; the marked text of an input method is underlined.
    pub(super) fn runs(
        &self,
        style: &TextStyle,
        len: usize,
        placeholder: bool,
        cx: &App,
    ) -> Vec<TextRun> {
        let color = if placeholder {
            cx.theme().colors().text_placeholder
        } else {
            style.color
        };
        let run = TextRun {
            len,
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let marked = self.model.marked().filter(|_| !placeholder);
        let Some(marked) = marked else {
            return vec![run];
        };
        let marked = self.to_display(marked.start)..self.to_display(marked.end);
        let underline = UnderlineStyle {
            color: Some(color),
            thickness: px(1.),
            wavy: false,
        };
        [
            TextRun {
                len: marked.start,
                ..run.clone()
            },
            TextRun {
                len: marked.len(),
                underline: Some(underline),
                ..run.clone()
            },
            TextRun {
                len: len.saturating_sub(marked.end),
                ..run
            },
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect()
    }

    /// An offset of the text in the drawn text (one `*` per char when masked).
    pub(super) fn to_display(&self, at: usize) -> usize {
        if self.masked {
            self.model.text()[..at.min(self.model.text().len())]
                .chars()
                .count()
        } else {
            at
        }
    }

    /// An offset of the drawn text in the text.
    pub(super) fn from_display(&self, at: usize) -> usize {
        let text = self.model.text();
        if text.is_empty() {
            0
        } else if self.masked {
            text.char_indices().nth(at).map_or(text.len(), |(index, _)| index)
        } else {
            motion::floor_boundary(text, at)
        }
    }

    /// The text laid out at the width the box last had; made again only when something it
    /// depends on changed.
    pub(super) fn layout(&mut self, window: &Window, cx: &App) -> Rc<Layout> {
        let width = match self.mode {
            Mode::OneLine => None,
            _ => self.last_bounds.map(|bounds| bounds.size.width),
        };
        let style = self.text_style(cx);
        let rem = window.rem_size();
        let shows_placeholder = self.model.text().is_empty() && !self.placeholder.is_empty();
        let key = LayoutKey {
            version: self.model.version(),
            width,
            font: style.font(),
            font_size: style.font_size.to_pixels(rem),
            line_height: style.line_height_in_pixels(rem),
            color: style.color,
            masked: self.masked,
            marked: self.model.marked(),
            placeholder: shows_placeholder.then(|| self.placeholder.clone()),
        };
        if let Some(laid) = &self.laid
            && laid.key == key
        {
            return laid.clone();
        }
        let (shown, is_placeholder) = self.display();
        let runs = self.runs(&style, shown.len(), is_placeholder, cx);
        let laid = Rc::new(Layout::build(key, &shown, &runs, window));
        self.laid = Some(laid.clone());
        laid
    }

    /// Scrolls so the caret is in view.
    pub(super) fn scroll_to_head(&mut self, laid: &Layout, bounds: Bounds<Pixels>) {
        let at = laid.position(self.to_display(self.model.head()), self.end_bias);
        if self.mode == Mode::OneLine {
            let width = bounds.size.width;
            if at.x + CARET_WIDTH - self.scroll.x > width {
                self.scroll.x = at.x + CARET_WIDTH - width;
            }
            if at.x < self.scroll.x {
                self.scroll.x = at.x;
            }
        } else {
            let height = bounds.size.height;
            if at.y < self.scroll.y {
                self.scroll.y = at.y;
            }
            if at.y + laid.line_height > self.scroll.y + height {
                self.scroll.y = at.y + laid.line_height - height;
            }
        }
    }

    /// Keeps the scroll inside the text.
    pub(super) fn clamp_scroll(&mut self, laid: &Layout, bounds: Bounds<Pixels>) {
        let zero = px(0.);
        if self.mode == Mode::OneLine {
            let most = (laid.width + CARET_WIDTH - bounds.size.width).max(zero);
            self.scroll = point(self.scroll.x.clamp(zero, most), zero);
        } else {
            let most = (laid.line_height * laid.rows as f32 - bounds.size.height).max(zero);
            self.scroll = point(zero, self.scroll.y.clamp(zero, most));
        }
    }

    /// The offset of the text under a window point, and whether it is a wrap point at a row's end.
    pub(super) fn index_at(
        &mut self,
        position: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> Option<(usize, bool)> {
        let bounds = self.last_bounds?;
        let laid = self.layout(window, cx);
        let (at, end_bias) = laid.index_at(position - bounds.origin + self.scroll);
        Some((self.from_display(at), end_bias))
    }

    /// The edges of the shown row that holds `at`, as offsets of the text.
    fn row_edges(&mut self, at: usize, window: &Window, cx: &App) -> (usize, usize) {
        let laid = self.layout(window, cx);
        let row = laid.row_of(self.to_display(at), self.end_bias);
        let range = laid.row_range(row);
        (self.from_display(range.start), self.from_display(range.end))
    }

    fn line_of(&self, at: usize) -> (usize, usize) {
        let text = self.model.text();
        (motion::line_start(text, at), motion::line_end(text, at))
    }

    // The keys (6.3.5).

    fn backspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.model.selection();
        let range = if selection.is_empty() {
            motion::previous_grapheme(self.model.text(), selection.head)..selection.head
        } else {
            selection.range()
        };
        self.delete_range(range, EditKind::Deleting, cx);
    }

    fn delete(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.model.selection();
        let range = if selection.is_empty() {
            selection.head..motion::next_grapheme(self.model.text(), selection.head)
        } else {
            selection.range()
        };
        self.delete_range(range, EditKind::Deleting, cx);
    }

    fn delete_word(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        if forward {
            self.delete_to(motion::next_word_end, cx);
        } else {
            self.delete_to(motion::previous_word_start, cx);
        }
    }

    fn delete_to_line_start(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(motion::line_start, cx);
    }

    /// The start or the end of the whole text.
    fn to_edge(&mut self, end: bool, select: bool, _: &mut Window, cx: &mut Context<Self>) {
        let at = if end { self.model.text().len() } else { 0 };
        self.move_caret(at, select, cx);
    }

    fn delete_range(&mut self, range: Range<usize>, kind: EditKind, cx: &mut Context<Self>) {
        if self.read_only || range.is_empty() {
            return;
        }
        self.edit(range, "", kind, cx);
    }

    /// Deletes the selection, or from the caret to `to`.
    fn delete_to(&mut self, to: impl Fn(&str, usize) -> usize, cx: &mut Context<Self>) {
        let selection = self.model.selection();
        let range = if selection.is_empty() {
            let to = to(self.model.text(), selection.head);
            to.min(selection.head)..to.max(selection.head)
        } else {
            selection.range()
        };
        self.delete_range(range, EditKind::Other, cx);
    }

    fn horizontal(&mut self, forward: bool, select: bool, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.model.selection();
        let text = self.model.text();
        let at = match (select || selection.is_empty(), forward) {
            (true, true) => motion::next_grapheme(text, selection.head),
            (true, false) => motion::previous_grapheme(text, selection.head),
            (false, true) => selection.range().end,
            (false, false) => selection.range().start,
        };
        self.move_caret(at, select, cx);
    }

    fn word(&mut self, forward: bool, select: bool, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.model.text();
        let head = self.model.head();
        let at = if forward {
            motion::next_word_end(text, head)
        } else {
            motion::previous_word_start(text, head)
        };
        self.move_caret(at, select, cx);
    }

    /// Home and End: they stop at the edge of the shown row of a wrapped line.
    fn row_edge(&mut self, end: bool, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        let head = self.model.head();
        let (start, row_end) = self.row_edges(head, window, cx);
        let wraps = row_end < self.line_of(head).1;
        self.move_caret(if end { row_end } else { start }, select, cx);
        self.end_bias = end && wraps;
    }

    /// Up and Down by shown rows, keeping the x the run of moves started at.
    fn vertical(&mut self, rows: isize, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        let laid = self.layout(window, cx);
        let head = self.to_display(self.model.head());
        let row = laid.row_of(head, self.end_bias) as isize;
        let goal = self
            .model
            .goal_x
            .map_or_else(|| laid.position(head, self.end_bias).x, px);
        let target = row + rows;
        let (at, end_bias) = if target < 0 {
            (0, false)
        } else if target as usize >= laid.rows {
            (self.model.text().len(), false)
        } else {
            let (at, end_bias) = laid.index_in_row(target as usize, goal);
            (self.from_display(at), end_bias)
        };
        self.move_caret(at, select, cx);
        self.end_bias = end_bias;
        self.model.goal_x = Some(goal.into());
    }

    /// The rows the box shows at once.
    fn page_rows(&mut self, window: &Window, cx: &App) -> isize {
        let laid = self.layout(window, cx);
        let height = self.last_bounds.map_or(px(0.), |bounds| bounds.size.height);
        ((height / laid.line_height) as isize).max(1)
    }

    fn page(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.page_rows(window, cx);
        self.vertical(if down { rows } else { -rows }, false, window, cx);
    }

    fn select_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.select_everything(cx);
    }

    /// The selected text, or the caret's whole line with its line end when nothing is selected
    /// (D10), and the range a cut takes.
    fn clip_range(&self) -> (String, Range<usize>) {
        let selection = self.model.selection();
        let text = self.model.text();
        if !selection.is_empty() {
            let range = selection.range();
            return (text[range.clone()].to_string(), range);
        }
        let (start, end) = self.line_of(selection.head);
        let line = &text[start..end];
        if end < text.len() {
            (format!("{line}\n"), start..end + 1)
        } else {
            (format!("{line}\n"), start..end)
        }
    }

    fn copy(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (text, _) = self.clip_range();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn cut(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (mut text, range) = self.clip_range();
        // A cut of the last line has no line end, as Zed's.
        if self.model.selection().is_empty() && range.end == self.model.text().len() {
            text.pop();
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if !self.read_only {
            self.edit(range, "", EditKind::Other, cx);
        }
    }

    fn paste(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let range = self.model.selection().range();
        self.edit(range, &text, EditKind::Other, cx);
    }

    fn undo(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !self.read_only {
            let edited = self.model.undo();
            self.changed(edited, cx);
        }
    }

    fn redo(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !self.read_only {
            let edited = self.model.redo();
            self.changed(edited, cx);
        }
    }

    fn newline(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !self.read_only {
            let range = self.model.selection().range();
            self.edit(range, "\n", EditKind::Typing, cx);
        }
    }

    /// Tab in a full box: spaces to the next tab stop.
    fn tab(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let range = self.model.selection().range();
        let start = motion::line_start(self.model.text(), range.start);
        let column = self.model.text()[start..range.start].chars().count();
        let spaces = " ".repeat(TAB_SIZE - column % TAB_SIZE);
        self.edit(range, &spaces, EditKind::Typing, cx);
    }

    /// Shift+Tab in a full box: up to one tab stop of spaces off the start of the line.
    fn backtab(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let text = self.model.text();
        let start = motion::line_start(text, self.model.head());
        let spaces = text[start..]
            .chars()
            .take(TAB_SIZE)
            .take_while(|c| *c == ' ')
            .count();
        if spaces > 0 {
            let selection = self.model.selection();
            self.edit(start..start + spaces, "", EditKind::Other, cx);
            let back = |at: usize| {
                if at >= start + spaces {
                    at - spaces
                } else {
                    at.min(start)
                }
            };
            self.model
                .select(back(selection.anchor), back(selection.head));
        }
    }

    // The mouse (6.3.3).

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A button over the box (a ButtonLike) keeps the caret where it is.
        if window.default_prevented() {
            return;
        }
        window.focus(&self.focus, cx);
        let Some((at, end_bias)) = self.index_at(event.position, window, cx) else {
            return;
        };
        let range = self.model.selection().range();
        let text = self.model.text();
        let mut inside = false;
        match event.click_count {
            1 if event.modifiers.shift => self.model.select_to(at),
            1 if !range.is_empty() && range.contains(&at) => inside = true,
            1 => self.model.move_to(at),
            2 => {
                let (start, end) = motion::word_around(text, at);
                self.model.select(start, end);
            }
            3 => {
                let (start, end) = (motion::line_start(text, at), motion::line_end(text, at));
                let end = if end < text.len() { end + 1 } else { end };
                self.model.select(start, end);
            }
            _ => {}
        }
        self.model.goal_x = None;
        self.press = Some(Press {
            at,
            time: self.now(cx),
            clicks: event.click_count,
            inside,
            moved: false,
            moving_text: false,
            origin: self.model.selection(),
        });
        self.changed(false, cx);
        self.end_bias = end_bias && !inside;
        cx.stop_propagation();
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.press.is_none() || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some((at, end_bias)) = self.index_at(event.position, window, cx) else {
            return;
        };
        let now = self.now(cx);
        let Some(press) = self.press.as_mut() else {
            return;
        };
        if press.inside && !press.moved {
            if now.saturating_duration_since(press.time) >= DRAG_DELAY {
                press.moving_text = true;
            } else {
                // Moved before the delay: a plain drag from the press.
                press.inside = false;
                press.origin = Selection::caret(press.at);
            }
        }
        press.moved = true;
        if press.moving_text {
            self.drop_at = Some(at);
            cx.notify();
            return;
        }
        let (clicks, origin) = (press.clicks, press.origin.range());
        let text = self.model.text();
        let (anchor, head) = match clicks {
            2 => {
                let (start, end) = motion::word_around(text, at);
                if at < origin.start {
                    (origin.end, start)
                } else {
                    (origin.start, end.max(origin.end))
                }
            }
            3 => {
                let start = motion::line_start(text, at);
                let end = motion::line_end(text, at);
                let end = if end < text.len() { end + 1 } else { end };
                if at < origin.start {
                    (origin.end, start)
                } else {
                    (origin.start, end.max(origin.end))
                }
            }
            _ => (origin.start, at),
        };
        self.model.select(anchor, head);
        self.model.goal_x = None;
        self.changed(false, cx);
        self.end_bias = end_bias;
    }

    pub(super) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(press) = self.press.take() else {
            return;
        };
        let drop_at = self.drop_at.take();
        if press.moving_text {
            let at = self
                .index_at(event.position, window, cx)
                .map(|(at, _)| at)
                .or(drop_at);
            let range = self.model.selection().range();
            match at {
                // Today's code moves text in a read-only box too (Bugs found, T-MOUSE-1).
                Some(at) if at < range.start || at > range.end => {
                    let copy = if cfg!(target_os = "macos") {
                        event.modifiers.alt
                    } else {
                        event.modifiers.control
                    };
                    let now = self.now(cx);
                    let edited = self.model.drop_text(range, at, copy, now);
                    self.changed(edited, cx);
                }
                Some(at) => self.move_caret(at, false, cx),
                None => cx.notify(),
            }
            return;
        }
        if press.inside && !press.moved {
            self.move_caret(press.at, false, cx);
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let laid = self.layout(window, cx);
        let delta = event.delta.pixel_delta(laid.line_height);
        let before = self.scroll;
        if self.mode == Mode::OneLine {
            self.scroll.x -= if delta.x == px(0.) { delta.y } else { delta.x };
        } else {
            self.scroll.y -= delta.y;
        }
        self.clamp_scroll(&laid, bounds);
        // A box that cannot scroll leaves the wheel to the list around it.
        if self.scroll != before {
            cx.stop_propagation();
            cx.notify();
        }
    }
}

/// Registers each own key action on the box: `Action => method(args)` calls
/// `method(args.., window, cx)`.
macro_rules! keys {
    ($div:expr, $cx:expr, $($action:ident => $method:ident($($arg:expr),*)),* $(,)?) => {
        $div$(.on_action($cx.listener(
            |input: &mut TextInput,
             _: &actions::$action,
             window: &mut Window,
             cx: &mut Context<TextInput>| {
                input.$method($($arg,)* window, cx)
            },
        )))*
    };
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut context = KeyContext::new_with_defaults();
        context.add("Editor");
        context.set("mode", self.mode.name());
        let root = div()
            .key_context(context)
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .map(|root| match self.mode {
                Mode::Full { .. } => root.size_full(),
                _ => root.w_full(),
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel));
        let root = keys!(root, cx,
            Backspace => backspace(),
            Delete => delete(),
            MoveLeft => horizontal(false, false),
            MoveRight => horizontal(true, false),
            SelectLeft => horizontal(false, true),
            SelectRight => horizontal(true, true),
            MoveToBeginningOfLine => row_edge(false, false),
            MoveToEndOfLine => row_edge(true, false),
            SelectToBeginningOfLine => row_edge(false, true),
            SelectToEndOfLine => row_edge(true, true),
            MoveToPreviousWordStart => word(false, false),
            MoveToNextWordEnd => word(true, false),
            SelectToPreviousWordStart => word(false, true),
            SelectToNextWordEnd => word(true, true),
            DeleteToPreviousWordStart => delete_word(false),
            DeleteToNextWordEnd => delete_word(true),
            DeleteToBeginningOfLine => delete_to_line_start(),
            SelectAll => select_all(),
            Copy => copy(),
            Cut => cut(),
            Paste => paste(),
            Undo => undo(),
            Redo => redo(),
            Newline => newline(),
            MoveUp => vertical(-1, false),
            MoveDown => vertical(1, false),
            SelectUp => vertical(-1, true),
            SelectDown => vertical(1, true),
            MovePageUp => page(false),
            MovePageDown => page(true),
            MoveToBeginning => to_edge(false, false),
            MoveToEnd => to_edge(true, false),
            SelectToBeginning => to_edge(false, true),
            SelectToEnd => to_edge(true, true),
            Tab => tab(),
            Backtab => backtab(),
        );
        root.child(TextElement::new(cx.entity()))
    }
}
