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
    Focusable, Font, Hsla, KeyContext, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Render, ScrollWheelEvent, SharedString, Subscription, TextRun, TextStyle,
    TextStyleRefinement, UnderlineStyle, Window, div, font, point, px, relative, rems,
};
use theme::ActiveTheme;

use super::blink::Blink;
use super::code_contrast::with_contrast;
use super::code_view::{Code, FAST_SCROLL};
use super::element::{CARET_WIDTH, Layout, LayoutKey, TextElement, text_inset, wrap_width};
use super::text_box::TextEvent;
use crate::app::appearance::Appearance;
use crate::components::form::input_style;
use crate::text::minimal::Edit;
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
            // The keys of the full boxes (`commands_editor.rs`).
            SelectNext,
            SelectAllMatches,
            SelectLargerSyntaxNode,
            SelectSmallerSyntaxNode,
            AddSelectionAbove,
            AddSelectionBelow,
            MoveLineUp,
            MoveLineDown,
            DuplicateLineDown,
            DeleteLine,
            ToggleComments,
            // Taken by the diff view around the box (`git/diff_view.rs`).
            ToggleSelectedDiffHunks,
        ]
    );
}

/// The kinds of box. The key context names them as Zed's editor did, so the bindings of
/// `text_boxes.rs` and `commands_editor.rs` still match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    OneLine,
    /// Grows from `min` to `max` lines with its text, then scrolls inside.
    Area {
        min: usize,
        max: usize,
    },
    /// Fills its box and scrolls inside; `current_line` paints the caret's row.
    Full {
        current_line: bool,
    },
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
/// The columns between two tab stops: a tab shows as spaces to the next one.
pub(super) const TAB_SIZE: usize = 4;
/// The clipboard metadata of a Copy or Cut that took the whole line (D10).
const WHOLE_LINE: &str = "natamux-whole-line";

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
    pub(super) masked: bool,
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
    pub(super) laid: Option<Rc<Layout>>,
    press: Option<Press>,
    /// Where held text would go in, while it is dragged.
    pub(super) drop_at: Option<usize>,
    /// What a code view has more (`code_view.rs`).
    pub(super) code: Option<Code>,
    /// The pinned row last painted as under the pointer, for tests.
    #[cfg(test)]
    pub(super) sticky_hovered: std::cell::Cell<Option<usize>>,
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
        Self::new(
            Mode::OneLine,
            placeholder,
            input_style(None, None),
            window,
            cx,
        )
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
        Self::full(true, read_only, window, cx)
    }

    /// A full text view in the buffer font; `current_line` paints the caret's row.
    pub fn full(
        current_line: bool,
        read_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = Mode::Full { current_line };
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
            code: None,
            #[cfg(test)]
            sticky_hovered: Default::default(),
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
        // The caret goes to the end, but the view does not scroll to it, as in Zed's editor (B1
        // shot `workspace-docs-edit` shows the top of the doc).
        self.autoscroll = false;
    }

    pub fn replace_minimal(&mut self, text: &str, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let edited = self.model.replace_minimal(text, now);
        if edited {
            Self::emit_edited(cx);
        }
        self.sync_code(cx);
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
            Self::emit_edited(cx);
        }
        self.sync_code(cx);
        self.refresh_brackets();
        cx.notify();
    }

    /// A click in a code view does not scroll, as `autoscroll_on_clicks` is off in Zed's editor;
    /// keys and edits still do, and a drag does once the pointer leaves the text.
    pub(super) fn no_click_scroll(&mut self) {
        if self.code.is_some() {
            self.autoscroll = false;
        }
    }

    /// Tells `Edited` one effect later, as Zed's editor did (its event came through the buffer):
    /// a subscription made right after `set_text` still hears it, and gpui turns a new
    /// subscription on only in a later effect. `LiveField` relies on that.
    fn emit_edited(cx: &mut Context<Self>) {
        let input = cx.weak_entity();
        cx.defer(move |cx| {
            input.update(cx, |_, cx| cx.emit(TextEvent::Edited)).ok();
        });
    }

    /// The rem size the text resolves its sizes with. A full box resolves them against its own
    /// font size, as Zed's editor did: a 12.5 px font given in rems draws at 12.5 x 12.5 / 16 px
    /// there (B1 shot `project-settings-stage-sheet`, the prompt).
    pub(super) fn rem_size(&self, style: &TextStyle, window: &Window) -> Pixels {
        let rem = window.rem_size();
        match self.mode {
            Mode::Full { .. } => style.font_size.to_pixels(rem),
            _ => rem,
        }
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
        } else if let Some(labels) = self.labels() {
            (SharedString::from(labels.insert(text)), false)
        } else {
            (SharedString::from(text.to_string()), false)
        }
    }

    /// The text `display` gives, borrowed when it is the text itself.
    pub(super) fn display_text(&self) -> std::borrow::Cow<'_, str> {
        let text = self.model.text();
        if text.is_empty() || self.masked || self.labels().is_some() {
            std::borrow::Cow::Owned(self.display().0.to_string())
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }

    /// One run of the text style, or the syntax colours of a code view, kept in contrast with
    /// the backgrounds of `marks`; the marked text of an input method is underlined.
    pub(super) fn runs(
        &self,
        style: &TextStyle,
        len: usize,
        placeholder: bool,
        marks: &[(Range<usize>, Hsla)],
        cx: &App,
    ) -> Vec<TextRun> {
        let color = if placeholder {
            cx.theme().colors().text_placeholder
        } else {
            style.color
        };
        let labels = self.labels().filter(|_| !placeholder);
        let text_len = len - labels.map_or(0, |labels| labels.total());
        let run = TextRun {
            len: text_len,
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if placeholder {
            None
        } else {
            let lines = self.query_lines();
            self.diff_runs(&run, text_len, lines.clone(), cx)
                .or_else(|| self.code_runs(&run, text_len, lines, &self.sticky_heads(), cx))
        };
        let label = labels.map(|_| self.label_run(&run, cx));
        let mut runs = runs.unwrap_or_else(|| vec![run]);
        if let (Some(labels), Some(label)) = (labels, label) {
            runs = labels.splice(runs, &label);
        }
        if !placeholder {
            runs = with_contrast(runs, marks, cx.theme().colors().editor_background);
        }
        let marked = self.model.marked().filter(|_| !placeholder);
        let Some(marked) = marked else {
            return runs;
        };
        let marked = self.to_display(marked.start)..self.to_display(marked.end);
        let underline = UnderlineStyle {
            color: Some(color),
            thickness: px(1.),
            wavy: false,
        };
        underline_range(runs, marked, underline)
    }

    /// An offset of the text in the drawn text (one `*` per char when masked).
    pub(super) fn to_display(&self, at: usize) -> usize {
        if self.masked {
            self.model.text()[..at.min(self.model.text().len())]
                .chars()
                .count()
        } else if let Some(labels) = self.labels() {
            labels.shown(at)
        } else {
            at
        }
    }

    /// An offset of the drawn text in the text.
    pub(super) fn text_offset(&self, at: usize) -> usize {
        let text = self.model.text();
        if text.is_empty() {
            0
        } else if self.masked {
            text.char_indices()
                .nth(at)
                .map_or(text.len(), |(index, _)| index)
        } else {
            let at = self.labels().map_or(at, |labels| labels.text_at(at));
            motion::floor_boundary(text, at)
        }
    }

    /// The text laid out at the width the box last had; made again only when something it
    /// depends on changed.
    pub(super) fn layout(&mut self, window: &Window, cx: &App) -> Rc<Layout> {
        let style = self.text_style(cx);
        let rem = self.rem_size(&style, window);
        let font = style.font();
        let font_size = style.font_size.to_pixels(rem);
        let room = self.bar_room(cx);
        let full = matches!(self.mode, Mode::Full { .. });
        let diff = self.code.as_ref().and_then(|code| code.diff.as_ref());
        let (gutter, gutter_pad) = if full && self.code.as_ref().is_some_and(|code| code.gutter) {
            match diff {
                Some(diff) => super::code_gutter::diff_gutter_size(
                    diff.widest_number(),
                    &font,
                    font_size,
                    window,
                ),
                None => {
                    let lines = self.model.text().split('\n').count();
                    super::code_gutter::gutter_size(lines, &font, font_size, window)
                }
            }
        } else {
            (px(0.), px(0.))
        };
        let (width, view, inset) = match self.mode {
            Mode::OneLine => (None, px(0.), px(0.)),
            _ => {
                let view = self.last_bounds.map(|bounds| {
                    let width = bounds.size.width - gutter;
                    (wrap_width(&font, font_size, width, window) - room).max(px(0.))
                });
                let width = if self.scrolls_sideways() { None } else { view };
                let inset = text_inset(&font, font_size, window) + gutter;
                (width, view.unwrap_or_default(), inset)
            }
        };
        let shows_placeholder = self.model.text().is_empty() && !self.placeholder.is_empty();
        let key = LayoutKey {
            version: self.model.version(),
            width,
            font,
            font_size,
            line_height: style.line_height_in_pixels(rem),
            color: style.color,
            masked: self.masked,
            marked: self.model.marked(),
            placeholder: shows_placeholder.then(|| self.placeholder.clone()),
            inset,
            colors: self.code.as_ref().map_or(0, |code| code.colors),
            gutter,
            gutter_pad,
            view,
            lines: self.query_lines(),
            heads: self.sticky_heads(),
            marks: self.contrast_marks(self.focus.is_focused(window), cx),
            blocks: diff.map(|diff| diff.rows.above.clone()),
        };
        if let Some(laid) = &self.laid
            && laid.key == key
        {
            return laid.clone();
        }
        let (shown, is_placeholder) = self.display();
        let runs = self.runs(&style, shown.len(), is_placeholder, &key.marks, cx);
        let laid = Rc::new(Layout::build(
            key,
            &shown,
            &runs,
            self.laid.as_deref(),
            window,
        ));
        self.laid = Some(laid.clone());
        laid
    }

    /// Scrolls so the caret is in view.
    pub(super) fn scroll_to_head(&mut self, laid: &Layout, bounds: Bounds<Pixels>) {
        let at = laid.position(self.to_display(self.model.head()), self.end_bias);
        if self.mode == Mode::OneLine {
            let width = bounds.size.width - laid.inset;
            if at.x + CARET_WIDTH - self.scroll.x > width {
                self.scroll.x = at.x + CARET_WIDTH - width;
            }
            if at.x < self.scroll.x {
                self.scroll.x = at.x;
            }
        } else {
            let height = bounds.size.height;
            let margin = self.code_margin(laid, height);
            if at.y - margin < self.scroll.y {
                self.scroll.y = at.y - margin;
            }
            if at.y + laid.line_height + margin > self.scroll.y + height {
                self.scroll.y = at.y + laid.line_height + margin - height;
            }
            // shortcut: no side margin, unlike Zed's horizontal autoscroll; match it when a
            // golden or shot of the editor tab shows the difference.
            if self.scrolls_sideways() {
                let room = laid.key.view;
                if at.x + CARET_WIDTH - self.scroll.x > room {
                    self.scroll.x = at.x + CARET_WIDTH - room;
                }
                if at.x < self.scroll.x {
                    self.scroll.x = at.x;
                }
            }
        }
    }

    /// Keeps the scroll inside the text.
    pub(super) fn clamp_scroll(&mut self, laid: &Layout, bounds: Bounds<Pixels>) {
        let zero = px(0.);
        if self.mode == Mode::OneLine {
            let most = (laid.width + CARET_WIDTH + laid.inset - bounds.size.width).max(zero);
            self.scroll = point(self.scroll.x.clamp(zero, most), zero);
        } else {
            let most = self.code_scroll_limit(laid).unwrap_or_else(|| {
                (laid.line_height * laid.rows as f32 - bounds.size.height).max(zero)
            });
            let most_x = self.code_scroll_x_limit(laid);
            self.scroll = point(
                self.scroll.x.clamp(zero, most_x),
                self.scroll.y.clamp(zero, most),
            );
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
        let text_origin = bounds.origin + point(laid.inset, px(0.));
        let (at, end_bias) = laid.index_at(position - text_origin + self.scroll);
        Some((self.text_offset(at), end_bias))
    }

    /// The edges of the shown row that holds `at`, as offsets of the text.
    fn row_edges(&mut self, at: usize, window: &Window, cx: &App) -> (usize, usize) {
        let laid = self.layout(window, cx);
        let row = laid.row_of(self.to_display(at), self.end_bias);
        let range = laid.row_range(row);
        (self.text_offset(range.start), self.text_offset(range.end))
    }

    fn line_of(&self, at: usize) -> (usize, usize) {
        let text = self.model.text();
        (motion::line_start(text, at), motion::line_end(text, at))
    }

    // The keys (6.3.5).

    fn backspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.delete_pair(cx) {
            return;
        }
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
        let extra = self.word_chars();
        if forward {
            self.delete_to(|text, at| motion::next_word_end_in(text, at, extra), cx);
        } else {
            self.delete_to(
                |text, at| motion::previous_word_start_in(text, at, extra),
                cx,
            );
        }
    }

    fn delete_to_line_start(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(motion::line_start, cx);
    }

    /// The start or the end of the whole text.
    fn edge(&mut self, end: bool, select: bool, _: &mut Window, cx: &mut Context<Self>) {
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
        let extra = self.word_chars();
        let at = if forward {
            motion::next_word_end_in(text, head, extra)
        } else {
            motion::previous_word_start_in(text, head, extra)
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

    /// Up and Down by shown rows, keeping the x the run of moves started at. Without Shift a
    /// selection is left from its start (up) or its end (down). A page stops at the first or the
    /// last row and keeps the x there.
    fn vertical(
        &mut self,
        rows: isize,
        select: bool,
        page: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let laid = self.layout(window, cx);
        let selection = self.model.selection();
        let (from, end_bias, goal) = if select || selection.is_empty() {
            (selection.head, self.end_bias, self.model.goal_x)
        } else if rows < 0 {
            (selection.range().start, false, None)
        } else {
            (selection.range().end, false, None)
        };
        let from = self.to_display(from);
        let row = laid.row_of(from, end_bias) as isize;
        let goal = goal.map_or_else(|| laid.position(from, end_bias).x, px);
        let last = laid.rows as isize - 1;
        let target = if page {
            (row + rows).clamp(0, last)
        } else {
            row + rows
        };
        let (at, end_bias) = if target < 0 {
            (0, false)
        } else if target > last {
            (self.model.text().len(), false)
        } else {
            let (at, end_bias) = laid.index_in_row(target as usize, goal);
            (self.text_offset(at), end_bias)
        };
        self.move_caret(at, select, cx);
        self.end_bias = end_bias;
        self.model.goal_x = Some(goal.into());
    }

    /// The rows a page key moves: the rows the box shows at once, less one.
    fn page_rows(&mut self, window: &Window, cx: &App) -> isize {
        let laid = self.layout(window, cx);
        let height = self.last_bounds.map_or(px(0.), |bounds| bounds.size.height);
        // One row less than the rows in view, as Zed's editor paged.
        ((height / laid.line_height) as isize - 1).max(1)
    }

    fn page(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.page_rows(window, cx);
        self.vertical(if down { rows } else { -rows }, false, true, window, cx);
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

    /// A clip of the selection, or of the whole line, marked so a paste puts it above the line.
    fn clip(&self, text: String) -> ClipboardItem {
        if self.model.selection().is_empty() {
            ClipboardItem::new_string_with_metadata(text, WHOLE_LINE.to_string())
        } else {
            ClipboardItem::new_string(text)
        }
    }

    fn copy(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (text, _) = self.clip_range();
        cx.write_to_clipboard(self.clip(text));
    }

    fn cut(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (mut text, range) = self.clip_range();
        // A cut of the last line has no line end, as Zed's.
        if self.model.selection().is_empty() && range.end == self.model.text().len() {
            text.pop();
        }
        cx.write_to_clipboard(self.clip(text));
        if !self.read_only {
            self.edit(range, "", EditKind::Other, cx);
        }
    }

    fn paste(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let Some(text) = item.text() else {
            return;
        };
        let selection = self.model.selection();
        if selection.is_empty() && item.metadata().is_some_and(|mark| mark == WHOLE_LINE) {
            // A whole line goes in above the caret's line; the caret stays on its text.
            let start = motion::line_start(self.model.text(), selection.head);
            let now = self.now(cx);
            let edited = self.model.edit(start..start, &text, EditKind::Paste, now);
            let len = crate::text::model::normalize(&text).len();
            self.model.move_to(selection.head + len);
            self.changed(edited, cx);
            return;
        }
        self.edit(selection.range(), &text, EditKind::Paste, cx);
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
        if !self.read_only && !self.newline_code(cx) {
            let range = self.model.selection().range();
            self.edit(range, "\n", EditKind::Typing, cx);
        }
    }

    /// Tab in a full box: spaces to the next tab stop, counted from the last tab. A selection on
    /// one line gets spaces to the next tab stop of the line's indent (a whole tab stop after a
    /// tab), at the selection when it starts inside the indent, else at the line start. A
    /// selection over more rows gives every line a whole tab stop at its start. The selection
    /// start always moves past the spaces, as does its end on an indented line. A language with
    /// hard tabs gets one tab char instead of the spaces.
    fn tab(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.model.selection();
        let text = self.model.text();
        if selection.is_empty() {
            let at = selection.head;
            let start = motion::line_start(text, at);
            let column = text[start..at]
                .rsplit('\t')
                .next()
                .unwrap_or("")
                .chars()
                .count();
            let fill = if self.typing().is_some_and(|typing| typing.hard_tabs) {
                "\t".to_owned()
            } else {
                " ".repeat(TAB_SIZE - column % TAB_SIZE)
            };
            self.edit(at..at, &fill, EditKind::Typing, cx);
            return;
        }
        let range = selection.range();
        let starts = self.line_starts(range.clone());
        let one_line = starts.len() == 1;
        let hard_tabs = self.typing().is_some_and(|typing| typing.hard_tabs);
        let edits: Vec<Edit> = starts
            .iter()
            .map(|&start| {
                let (len, tabs) = indent_of(&text[start..]);
                let spaces = if tabs || !one_line {
                    TAB_SIZE
                } else {
                    TAB_SIZE - len % TAB_SIZE
                };
                let at = if one_line && len >= range.start - start {
                    range.start
                } else {
                    start
                };
                Edit {
                    range: at..at,
                    text: if hard_tabs {
                        "\t".to_owned()
                    } else {
                        " ".repeat(spaces)
                    },
                }
            })
            .collect();
        let moved = |at: usize| crate::text::minimal::map_offset(&edits, at);
        let (anchor, head) = (moved(selection.anchor), moved(selection.head));
        self.edit_all(&edits, anchor, head, cx);
    }

    /// Shift+Tab in a full box: each line the selection touches goes back to the tab stop before
    /// its indent, or loses its leading tab.
    fn backtab(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.model.selection();
        let text = self.model.text();
        let edits: Vec<Edit> = self
            .line_starts(selection.range())
            .into_iter()
            .filter_map(|start| {
                let (len, tabs) = indent_of(&text[start..]);
                let remove = match (tabs, len % TAB_SIZE) {
                    (true, _) => len.min(1),
                    (false, 0) => len.min(TAB_SIZE),
                    (false, rest) => rest,
                };
                (remove > 0).then(|| Edit {
                    range: start..start + remove,
                    text: String::new(),
                })
            })
            .collect();
        if edits.is_empty() {
            return;
        }
        let moved = |at: usize| crate::text::minimal::map_offset(&edits, at);
        let (anchor, head) = (moved(selection.anchor), moved(selection.head));
        self.edit_all(&edits, anchor, head, cx);
    }

    pub(super) fn edit_all(
        &mut self,
        edits: &[Edit],
        anchor: usize,
        head: usize,
        cx: &mut Context<Self>,
    ) {
        let now = self.now(cx);
        self.model.edit_all(edits, anchor, head, now);
        self.changed(true, cx);
    }

    /// The lines `range` touches, from the start of its first to the end of its last, without
    /// the line end. A selection that ends at a line start leaves that line out.
    pub(super) fn line_block(&self, range: Range<usize>) -> Range<usize> {
        let text = self.model.text();
        let end = if range.end > range.start && motion::line_start(text, range.end) == range.end {
            range.end - 1
        } else {
            range.end
        };
        motion::line_start(text, range.start)..motion::line_end(text, end)
    }

    /// The start of each line `range` touches.
    fn line_starts(&self, range: Range<usize>) -> Vec<usize> {
        let block = self.line_block(range);
        let text = self.model.text();
        std::iter::once(block.start)
            .chain(
                text[block.clone()]
                    .match_indices('\n')
                    .map(|(at, _)| block.start + at + 1),
            )
            .collect()
    }

    // The keys of the full boxes (`commands_editor.rs`). One selection only (D11): the keys that
    // add selections, and the ones that need a language, do nothing in plain text.

    fn nothing(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    /// With nothing selected, selects the word at the caret.
    fn select_word(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.model.selection();
        if !selection.is_empty() {
            return;
        }
        let (start, end) =
            motion::word_around_in(self.model.text(), selection.head, self.word_chars());
        if start < end {
            self.model.select(start, end);
            self.model.goal_x = None;
            self.changed(false, cx);
        }
    }

    /// Moves the lines of the selection past the line below (`down`) or above, as one step.
    fn move_lines(&mut self, down: bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.model.selection();
        let block = self.line_block(selection.range());
        let text = self.model.text();
        let lines = &text[block.clone()];
        let (edit, shift) = if down {
            if block.end >= text.len() {
                return;
            }
            let below = block.end + 1..motion::line_end(text, block.end + 1);
            let edit = Edit {
                range: block.start..below.end,
                text: format!("{}\n{lines}", &text[below.clone()]),
            };
            (edit, below.len() as isize + 1)
        } else {
            if block.start == 0 {
                return;
            }
            let above = motion::line_start(text, block.start - 1)..block.start - 1;
            let edit = Edit {
                range: above.start..block.end,
                text: format!("{lines}\n{}", &text[above.clone()]),
            };
            (edit, -(above.len() as isize + 1))
        };
        let moved = |at: usize| (at as isize + shift) as usize;
        let (anchor, head) = (moved(selection.anchor), moved(selection.head));
        self.edit_all(&[edit], anchor, head, cx);
    }

    /// Copies the lines of the selection below them; the selection goes with the lower copy.
    fn duplicate_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.model.selection();
        let block = self.line_block(selection.range());
        let copy = format!("{}\n", &self.model.text()[block.clone()]);
        let len = copy.len();
        let edit = Edit {
            range: block.start..block.start,
            text: copy,
        };
        let (anchor, head) = (selection.anchor + len, selection.head + len);
        self.edit_all(&[edit], anchor, head, cx);
    }

    /// Deletes the lines of the selection; the caret keeps its column on the line that takes
    /// their place.
    fn delete_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.model.selection();
        let block = self.line_block(selection.range());
        let text = self.model.text();
        let head = selection.head;
        let column = text[motion::line_start(text, head)..head].chars().count();
        let range = if block.end < text.len() {
            block.start..block.end + 1
        } else {
            block.start.saturating_sub(1)..block.end
        };
        let mut after = text.to_string();
        after.replace_range(range.clone(), "");
        let start = motion::line_start(&after, range.start.min(after.len()));
        let end = motion::line_end(&after, start);
        let caret = after[start..end]
            .char_indices()
            .nth(column)
            .map_or(end, |(at, _)| start + at);
        let edit = Edit {
            range,
            text: String::new(),
        };
        self.edit_all(&[edit], caret, caret, cx);
    }

    // The mouse (6.3.3).

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A button over the box (a ButtonLike) keeps the caret where it is.
        if window.default_prevented() {
            return;
        }
        window.focus(&self.focus, cx);
        if self.bar_press(event.position, window, cx) {
            cx.stop_propagation();
            return;
        }
        // A diff side's line numbers take no press; its strips are left to the diff view.
        match self.diff_gutter_press(event.position, window, cx) {
            Some(super::diff_side::GutterPress::Number) => {
                cx.stop_propagation();
                return;
            }
            Some(super::diff_side::GutterPress::Strip) => return,
            None => {}
        }
        // A press on a pinned row puts a caret on its line, whatever the clicks and keys.
        let pinned = self.sticky_press(event.position, window, cx);
        let Some((at, end_bias)) = pinned
            .map(|at| (at, false))
            .or_else(|| self.index_at(event.position, window, cx))
        else {
            return;
        };
        let range = self.model.selection().range();
        let text = self.model.text();
        let mut inside = false;
        // A press on the line numbers takes whole lines, as a triple click does.
        let in_gutter = pinned.is_none() && self.in_gutter(event.position);
        let clicks = match (pinned, in_gutter) {
            (Some(_), _) => 1,
            (None, true) => 3,
            (None, false) => event.click_count,
        };
        match clicks {
            _ if pinned.is_some() => self.model.move_to(at),
            _ if in_gutter && event.modifiers.shift => {
                let (start, end) = (motion::line_start(text, at), motion::line_end(text, at));
                let end = if end < text.len() { end + 1 } else { end };
                let anchor = self.model.selection().anchor;
                self.model
                    .select(anchor, if start >= anchor { end } else { start });
            }
            1 if event.modifiers.shift => self.model.select_to(at),
            1 if !range.is_empty() && range.contains(&at) => inside = true,
            1 => self.model.move_to(at),
            2 => {
                let (start, end) = motion::word_around_in(text, at, self.word_chars());
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
            clicks,
            inside,
            moved: false,
            moving_text: false,
            origin: self.model.selection(),
        });
        self.changed(false, cx);
        self.no_click_scroll();
        self.end_bias = end_bias && !inside;
        cx.stop_propagation();
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bar_drag(event.position, window, cx) {
            return;
        }
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
                let (start, end) = motion::word_around_in(text, at, self.word_chars());
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
        // Zed scrolls a drag only while the pointer is above or below the text.
        let inside = self.last_bounds.is_some_and(|bounds| {
            event.position.y >= bounds.top() && event.position.y < bounds.bottom()
        });
        if inside {
            self.no_click_scroll();
        }
        self.end_bias = end_bias;
    }

    pub(super) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bar_release(cx) {
            return;
        }
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
            // A diff side's text never changes, as the old diff's read-only buffers did not: the
            // drop only puts the caret there (T-MOUSE-1, diff cases).
            let fixed = self.code.as_ref().is_some_and(|code| code.diff.is_some());
            match at {
                // Today's code moves text in a read-only box too (Bugs found, T-MOUSE-1).
                Some(at) if !fixed && (at < range.start || at > range.end) => {
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
            self.no_click_scroll();
        }
    }

    fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let laid = self.layout(window, cx);
        let mut delta = event.delta.pixel_delta(laid.line_height);
        if self.code.is_some() && event.modifiers.alt {
            delta = point(delta.x * FAST_SCROLL, delta.y * FAST_SCROLL);
        }
        let before = self.scroll;
        if self.mode == Mode::OneLine {
            self.scroll.x -= if delta.x == px(0.) { delta.y } else { delta.x };
        } else {
            self.scroll.y -= delta.y;
            // Kept at 0 unless the lines do not wrap (`clamp_scroll`).
            self.scroll.x -= delta.x;
        }
        self.clamp_scroll(&laid, bounds);
        // A box that cannot scroll leaves the wheel to the list around it.
        if self.scroll != before {
            self.code_scrolled(cx);
            cx.stop_propagation();
            cx.notify();
        }
    }
}

/// `runs` with `range` underlined, split where it starts and ends.
fn underline_range(
    runs: Vec<TextRun>,
    range: Range<usize>,
    underline: UnderlineStyle,
) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        let cuts = [
            start,
            range.start.clamp(start, end),
            range.end.clamp(start, end),
            end,
        ];
        for part in cuts.windows(2) {
            if part[1] > part[0] {
                let inside = part[0] >= range.start && part[1] <= range.end;
                out.push(TextRun {
                    len: part[1] - part[0],
                    underline: if inside {
                        Some(underline)
                    } else {
                        run.underline
                    },
                    ..run.clone()
                });
            }
        }
    }
    out
}

/// The indent of the line that starts `line`: its count of leading spaces and tabs, and
/// whether it starts with a tab.
fn indent_of(line: &str) -> (usize, bool) {
    let len = line
        .bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count();
    (len, line.starts_with('\t'))
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
        context.add(crate::app::text_boxes::OWN_INPUT);
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
            .on_mouse_down(MouseButton::Right, cx.listener(Self::right_down))
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
            MoveUp => vertical(-1, false, false),
            MoveDown => vertical(1, false, false),
            SelectUp => vertical(-1, true, false),
            SelectDown => vertical(1, true, false),
            MovePageUp => page(false),
            MovePageDown => page(true),
            MoveToBeginning => edge(false, false),
            MoveToEnd => edge(true, false),
            SelectToBeginning => edge(false, true),
            SelectToEnd => edge(true, true),
            Tab => tab(),
            Backtab => backtab(),
            SelectNext => select_word(),
            SelectAllMatches => select_word(),
            SelectLargerSyntaxNode => grow_selection(),
            SelectSmallerSyntaxNode => shrink_selection(),
            AddSelectionAbove => nothing(),
            AddSelectionBelow => nothing(),
            MoveLineUp => move_lines(false),
            MoveLineDown => move_lines(true),
            DuplicateLineDown => duplicate_lines(),
            DeleteLine => delete_lines(),
            ToggleComments => toggle_comments(),
        );
        root.child(TextElement::new(cx.entity()))
            .children(self.menu_element())
    }
}
