//! The rendered view of a stage file (`Markdown.tsx`), Natamux's own (plan 6.6): the events of
//! pulldown-cmark drawn as gpui blocks and text, in the look of the Markdown preview the view
//! had before. No HTML, no Mermaid and no images (an image shows its alt text); only web and
//! mail links open. A mouse selection is look only: no key copies from it (D13).

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    BorderStyle, Bounds, CornersRefinement, CursorStyle, DispatchPhase, Edges, EdgesRefinement,
    Entity, FocusHandle, FontStyle, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InputHandler, InspectorElementId, KeyContext, LayoutId, Length, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Point, ScrollHandle, Stateful, StrikethroughStyle, Style,
    StyleRefinement, StyledText, TextAlign, TextLayout, TextRun, TextStyle, TextStyleRefinement,
    UTF16Selection, UnderlineStyle, WrappedLineLayout, point, quad, size,
};
use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd,
};
use settings::Settings as _;
use theme_settings::ThemeSettings;
use ui::{Checkbox, ScrollAxes, Scrollbars, WithScrollbar, prelude::*};

use crate::app::links::open_link;

/// GitHub's extras (tables, task lists, strikethrough, alerts), footnotes, smart punctuation,
/// heading attributes, superscript, subscript and `+++` metadata blocks (dropped, not drawn).
const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_SMART_PUNCTUATION)
    .union(Options::ENABLE_HEADING_ATTRIBUTES)
    .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS)
    .union(Options::ENABLE_OLD_FOOTNOTES)
    .union(Options::ENABLE_GFM)
    .union(Options::ENABLE_SUPERSCRIPT)
    .union(Options::ENABLE_SUBSCRIPT);

/// One view per shown file; a new text goes in with `set_text`.
pub struct MarkdownDoc {
    state: Entity<DocState>,
}

impl MarkdownDoc {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: cx.new(|cx| DocState {
                parsed: Rc::default(),
                selection: Selection::default(),
                pressed_link: None,
                pressed_note: None,
                autoscroll: None,
                focus: cx.focus_handle(),
                code_scrolls: BTreeMap::new(),
            }),
        }
    }

    /// The same text does nothing, so the view does not parse or redraw.
    pub fn set_text(&self, text: &str, cx: &mut App) {
        if self.state.read(cx).parsed.source.as_ref() == text {
            return;
        }
        let parsed = Rc::new(Parsed::new(SharedString::from(text.to_owned())));
        self.state.update(cx, |state, cx| {
            state.parsed = parsed;
            cx.notify();
        });
    }

    pub fn view(&self, window: &Window, cx: &App) -> AnyElement {
        let body = DocElement {
            state: self.state.clone(),
            look: look(window, cx),
        };
        div()
            .id("docs-markdown")
            .size_full()
            .min_h_0()
            .p_4()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }
}

// Parsing: the doc as a list of steps, made once per text.

/// One step of a parsed doc. Its source range goes with it.
enum Piece {
    Start(Tag<'static>),
    End(TagEnd),
    /// Text as shown. `plain` when it is the source text itself, with no entity or smart
    /// punctuation in it.
    Text {
        shown: String,
        plain: bool,
    },
    /// Inline code as shown: the source between its backticks.
    Code(String),
    Html,
    InlineHtml,
    FootnoteRef(SharedString),
    SoftBreak,
    HardBreak,
    Rule,
    TaskMarker(bool),
}

#[derive(Default)]
struct Parsed {
    source: SharedString,
    pieces: Vec<(Range<usize>, Piece)>,
    /// Where the text of each footnote starts, for a click on a reference to it.
    note_starts: HashMap<SharedString, usize>,
}

impl Parsed {
    fn new(source: SharedString) -> Self {
        let pieces = if source.is_empty() {
            Vec::new()
        } else {
            pieces(&source)
        };
        let note_starts = note_starts(&pieces);
        Self {
            source,
            pieces,
            note_starts,
        }
    }

    /// The end of the last step. A block that reaches it gets no bottom margin.
    fn end(&self) -> usize {
        self.pieces.last().map_or(0, |(range, _)| range.end)
    }
}

fn pieces(source: &str) -> Vec<(Range<usize>, Piece)> {
    let mut out = Vec::new();
    let mut events = Parser::new_ext(source, OPTIONS)
        .into_offset_iter()
        .peekable();
    let (mut in_link, mut in_code, mut in_table, mut in_metadata) = (false, false, false, false);
    while let Some((event, range)) = events.next() {
        if in_metadata {
            in_metadata = !matches!(event, Event::End(TagEnd::MetadataBlock(_)));
            continue;
        }
        match event {
            Event::Start(Tag::MetadataBlock(_)) => in_metadata = true,
            Event::Start(tag) => {
                match &tag {
                    Tag::Link { .. } => in_link = true,
                    Tag::CodeBlock(_) => in_code = true,
                    Tag::Table(_) => in_table = true,
                    _ => {}
                }
                out.push((range, Piece::Start(tag.into_static())));
            }
            Event::End(end) => {
                match end {
                    TagEnd::Link => in_link = false,
                    TagEnd::CodeBlock => in_code = false,
                    TagEnd::Table => in_table = false,
                    _ => {}
                }
                out.push((range, Piece::End(end)));
            }
            Event::Text(text) if in_code => out.push(text_piece(source, range, text.into_string())),
            Event::Text(text) => {
                let mut run = vec![(range, text.into_string())];
                while let Some((Event::Text(_), _)) = events.peek() {
                    if let Some((Event::Text(next), next_range)) = events.next() {
                        run.push((next_range, next.into_string()));
                    }
                }
                push_run(source, run, !in_link, &mut out);
            }
            Event::Code(parsed) => {
                let inner = code_content(&source[range.clone()]);
                let content = range.start + inner.start..range.start + inner.end;
                let text = &source[content.clone()];
                // A `\|` in a table cell is the cell's own escape: show what it stands for.
                let shown = if in_table && text.contains("\\|") {
                    parsed.into_string()
                } else {
                    text.to_owned()
                };
                out.push((content, Piece::Code(shown)));
            }
            Event::Html(_) => out.push((range, Piece::Html)),
            Event::InlineHtml(_) => out.push((range, Piece::InlineHtml)),
            Event::FootnoteReference(label) => {
                out.push((range, Piece::FootnoteRef(label.to_string().into())));
            }
            Event::SoftBreak => out.push((range, Piece::SoftBreak)),
            Event::HardBreak => out.push((range, Piece::HardBreak)),
            Event::Rule => out.push((range, Piece::Rule)),
            Event::TaskListMarker(done) => out.push((range, Piece::TaskMarker(done))),
            Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }
    out
}

fn text_piece(source: &str, range: Range<usize>, shown: String) -> (Range<usize>, Piece) {
    let plain = source.get(range.clone()) == Some(shown.as_str());
    (range, Piece::Text { shown, plain })
}

/// A part of a run of text: its source range and where it sits in the joined run.
struct Part {
    range: Range<usize>,
    from: usize,
    to: usize,
    shown: String,
}

impl Part {
    /// Cuts off the first `cut` bytes as a part of their own.
    fn split_head(&mut self, cut: usize) -> Part {
        let tail = self.shown.split_off(cut);
        let head = std::mem::replace(&mut self.shown, tail);
        let part = Part {
            range: self.range.start..self.range.start + cut,
            from: self.from,
            to: self.from + cut,
            shown: head,
        };
        self.range.start += cut;
        self.from += cut;
        part
    }

    fn piece(self, source: &str) -> (Range<usize>, Piece) {
        text_piece(source, self.range, self.shown)
    }
}

/// Text that follows text is joined, so a bare URL is found across smart quotes and entities.
/// Each URL becomes a link around the parts it covers, cut where it starts and ends.
fn push_run(
    source: &str,
    run: Vec<(Range<usize>, String)>,
    find_links: bool,
    out: &mut Vec<(Range<usize>, Piece)>,
) {
    if !find_links {
        out.extend(
            run.into_iter()
                .map(|(range, shown)| text_piece(source, range, shown)),
        );
        return;
    }
    let joined: String = run.iter().map(|(_, shown)| shown.as_str()).collect();
    let mut at = 0;
    let mut parts: VecDeque<Part> = run
        .into_iter()
        .map(|(range, shown)| {
            let from = at;
            at += shown.len();
            Part {
                range,
                from,
                to: at,
                shown,
            }
        })
        .collect();
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    for link in finder.links(&joined) {
        while parts.front().is_some_and(|part| part.to <= link.start()) {
            if let Some(part) = parts.pop_front() {
                out.push(part.piece(source));
            }
        }
        let Some(first) = parts.front_mut() else {
            continue;
        };
        let cut = link.start().saturating_sub(first.from);
        if cut > 0 && first.shown.is_char_boundary(cut) {
            out.push(first.split_head(cut).piece(source));
        }
        let start = first.range.start;
        let mut end = first.range.end;
        let mut inside = Vec::new();
        while parts.front().is_some_and(|part| part.to <= link.end()) {
            if let Some(part) = parts.pop_front() {
                end = part.range.end;
                inside.push(part.piece(source));
            }
        }
        if let Some(next) = parts.front_mut() {
            let cut = link.end().saturating_sub(next.from);
            if cut > 0 && next.shown.is_char_boundary(cut) {
                inside.push(next.split_head(cut).piece(source));
                end = next.range.start;
            }
        }
        let tag = Tag::Link {
            link_type: LinkType::Autolink,
            dest_url: link.as_str().to_owned().into(),
            title: "".into(),
            id: "".into(),
        };
        out.push((start..end, Piece::Start(tag)));
        out.extend(inside);
        out.push((start..end, Piece::End(TagEnd::Link)));
    }
    out.extend(parts.into_iter().map(|part| part.piece(source)));
}

/// The part of an inline code span between its backticks, when both ends have as many.
fn code_content(text: &str) -> Range<usize> {
    let open = text.bytes().take_while(|byte| *byte == b'`').count();
    let close = text.bytes().rev().take_while(|byte| *byte == b'`').count();
    if open == 0 || open != close || text.len() < open + close {
        return 0..text.len();
    }
    open..text.len() - close
}

/// The first plain text of each footnote.
fn note_starts(pieces: &[(Range<usize>, Piece)]) -> HashMap<SharedString, usize> {
    let mut starts = HashMap::new();
    let mut open: Option<SharedString> = None;
    for (range, piece) in pieces {
        match piece {
            Piece::Start(Tag::FootnoteDefinition(label)) => open = Some(label.to_string().into()),
            Piece::End(TagEnd::FootnoteDefinition) => open = None,
            Piece::Text { plain: true, .. } => {
                if let Some(label) = open.take() {
                    starts.entry(label).or_insert(range.start);
                }
            }
            _ => {}
        }
    }
    starts
}

// The look: the preview style of the old view, on the app's text scale.

#[derive(Clone, Default)]
struct Look {
    base: TextStyle,
    /// The font size of the doc's outer block.
    root_size: Option<AbsoluteLength>,
    code_block: StyleRefinement,
    inline_code: TextStyleRefinement,
    quote: TextStyleRefinement,
    link: TextStyleRefinement,
    heading: StyleRefinement,
    /// h1 to h6.
    levels: [TextStyleRefinement; 6],
    paragraph_spacing: Pixels,
    paragraph_line_height: DefiniteLength,
    /// The bottom margin of a top-level list.
    list_spacing: Pixels,
    cell_padding: Point<Pixels>,
    text: Hsla,
    border: Hsla,
    border_variant: Hsla,
    code_background: Hsla,
    head_background: Hsla,
    stripe_background: Hsla,
    selection: Hsla,
    /// Note, tip, important, warning and caution alerts.
    alerts: [Hsla; 5],
}

fn look(window: &Window, cx: &App) -> Look {
    let settings = ThemeSettings::get_global(cx);
    let colors = cx.theme().colors();
    let status = cx.theme().status();
    let mut base = window.text_style();
    base.refine(&TextStyleRefinement {
        font_family: Some(settings.markdown_preview_font_family().clone()),
        font_fallbacks: settings.ui_font.fallbacks.clone(),
        font_features: Some(settings.ui_font.features.clone()),
        ..Default::default()
    });
    base.color = colors.text;
    base.line_height = relative(1.5);
    let buffer = &settings.buffer_font;
    let code_font = settings.markdown_preview_code_font_family().clone();
    let px_edges = |value: f32| EdgesRefinement {
        top: Some(px(value).into()),
        right: Some(px(value).into()),
        bottom: Some(px(value).into()),
        left: Some(px(value).into()),
    };
    let border_one = Some(AbsoluteLength::Pixels(px(1.)));
    let corner = Some(AbsoluteLength::Pixels(px(6.)));
    let weight = settings.markdown_preview_heading_font_weight();
    let level = |color: Option<Hsla>| TextStyleRefinement {
        font_weight: Some(weight),
        line_height: Some(relative(1.25)),
        color,
        ..Default::default()
    };
    let mut look = Look {
        base,
        root_size: None,
        code_block: StyleRefinement {
            padding: px_edges(12.),
            margin: EdgesRefinement {
                top: Some(Length::Definite(px(16.).into())),
                right: Some(Length::Definite(px(0.).into())),
                bottom: Some(Length::Definite(px(16.).into())),
                left: Some(Length::Definite(px(0.).into())),
            },
            border_style: Some(BorderStyle::Solid),
            border_widths: EdgesRefinement {
                top: border_one,
                right: border_one,
                bottom: border_one,
                left: border_one,
            },
            border_color: Some(colors.border_variant),
            background: Some(colors.editor_background.into()),
            corner_radii: CornersRefinement {
                top_left: corner,
                top_right: corner,
                bottom_right: corner,
                bottom_left: corner,
            },
            text: TextStyleRefinement {
                font_family: Some(code_font.clone()),
                font_fallbacks: buffer.fallbacks.clone(),
                font_features: Some(buffer.features.clone()),
                font_weight: Some(buffer.weight),
                line_height: Some(relative(settings.buffer_line_height.value())),
                ..Default::default()
            },
            ..Default::default()
        },
        inline_code: TextStyleRefinement {
            font_family: Some(code_font),
            font_fallbacks: buffer.fallbacks.clone(),
            font_features: Some(buffer.features.clone()),
            font_weight: Some(buffer.weight),
            background_color: Some(colors.editor_foreground.opacity(0.08)),
            color: Some(colors.text),
            ..Default::default()
        },
        quote: TextStyleRefinement {
            color: Some(colors.text_muted),
            ..Default::default()
        },
        link: TextStyleRefinement {
            color: Some(colors.text_accent),
            underline: Some(UnderlineStyle {
                color: Some(colors.text_accent.opacity(0.5)),
                thickness: px(1.),
                ..Default::default()
            }),
            ..Default::default()
        },
        heading: StyleRefinement {
            margin: EdgesRefinement {
                top: Some(Length::Definite(px(24.).into())),
                bottom: Some(Length::Definite(px(12.).into())),
                ..Default::default()
            },
            text: TextStyleRefinement {
                color: Some(colors.text),
                ..Default::default()
            },
            ..Default::default()
        },
        levels: [
            level(None),
            level(None),
            level(None),
            level(None),
            level(None),
            level(Some(colors.text_muted)),
        ],
        paragraph_spacing: px(16.),
        paragraph_line_height: relative(1.5),
        list_spacing: px(12.),
        cell_padding: point(px(10.), px(4.)),
        text: colors.text,
        border: colors.border,
        border_variant: colors.border_variant,
        code_background: colors.editor_background,
        head_background: colors.title_bar_background,
        stripe_background: colors.panel_background,
        selection: colors.element_selection_background,
        alerts: [
            status.info,
            status.success,
            status.info,
            status.warning,
            status.error,
        ],
    };
    doc_text_sizes(&mut look);
    look
}

/// The doc on the app's text scale (`text_size.rs`, 13 px body) with the web's heading steps.
/// Rems keep the UI zoom.
fn doc_text_sizes(look: &mut Look) {
    let body: AbsoluteLength = rems_from_px(13.0_f32).into();
    look.base.font_size = body;
    look.root_size = Some(body);
    look.code_block.text.font_size = Some(rems_from_px(13.0_f32).into());
    look.inline_code.font_size = Some(rems_from_px(12.0_f32).into());
    for (level, size) in look
        .levels
        .iter_mut()
        .zip([20.0_f32, 16.0, 15.0, 13.0, 13.0, 12.0])
    {
        level.font_size = Some(rems_from_px(size).into());
    }
}

fn alert_color(look: &Look, kind: BlockQuoteKind) -> Hsla {
    let [note, tip, important, warning, caution] = look.alerts;
    match kind {
        BlockQuoteKind::Note => note,
        BlockQuoteKind::Tip => tip,
        BlockQuoteKind::Important => important,
        BlockQuoteKind::Warning => warning,
        BlockQuoteKind::Caution => caution,
    }
}

fn alert_header(kind: BlockQuoteKind, color: Hsla) -> AnyElement {
    let (icon, label) = match kind {
        BlockQuoteKind::Note => (IconName::Info, "Note"),
        BlockQuoteKind::Tip => (IconName::Sparkle, "Tip"),
        BlockQuoteKind::Important => (IconName::Chat, "Important"),
        BlockQuoteKind::Warning => (IconName::Warning, "Warning"),
        BlockQuoteKind::Caution => (IconName::Stop, "Caution"),
    };
    h_flex()
        .gap_1()
        .items_center()
        .mb_1()
        .child(
            Icon::new(icon)
                .size(IconSize::Small)
                .color(Color::Custom(color)),
        )
        .child(
            Label::new(label)
                .color(Color::Custom(color))
                .weight(FontWeight::BOLD),
        )
        .into_any_element()
}

fn text_align_of(alignment: Alignment) -> Option<TextAlign> {
    match alignment {
        Alignment::Left => Some(TextAlign::Left),
        Alignment::Center => Some(TextAlign::Center),
        Alignment::Right => Some(TextAlign::Right),
        Alignment::None => None,
    }
}

fn aligned(block: Div, align: TextAlign) -> Div {
    match align {
        TextAlign::Left => block.text_left(),
        TextAlign::Center => block.text_center(),
        TextAlign::Right => block.text_right(),
    }
}

fn level_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

// Building: the steps of the doc turned into nested blocks, each frame.

/// A block being filled, with or without an element id.
enum Holder {
    Plain(Div),
    Keyed(Stateful<Div>),
}

impl From<Div> for Holder {
    fn from(block: Div) -> Self {
        Self::Plain(block)
    }
}

impl From<Stateful<Div>> for Holder {
    fn from(block: Stateful<Div>) -> Self {
        Self::Keyed(block)
    }
}

impl Holder {
    fn style(&mut self) -> &mut StyleRefinement {
        match self {
            Self::Plain(block) => block.style(),
            Self::Keyed(block) => block.style(),
        }
    }

    fn push(&mut self, child: AnyElement) {
        match self {
            Self::Plain(block) => block.extend([child]),
            Self::Keyed(block) => block.extend([child]),
        }
    }

    fn into_any(self) -> AnyElement {
        match self {
            Self::Plain(block) => block.into_any_element(),
            Self::Keyed(block) => block.into_any_element(),
        }
    }
}

/// Shown byte `shown` of a line is source byte `source`, and so on up to the next mapping.
#[derive(Clone, Copy)]
struct Mapping {
    shown: usize,
    source: usize,
}

/// The text of the line being filled.
#[derive(Default)]
struct Draft {
    text: String,
    runs: Vec<TextRun>,
    map: Vec<Mapping>,
    /// Inline code, in shown bytes.
    chips: Vec<(Range<usize>, Hsla)>,
}

/// A link or a footnote reference: what a click on its source range opens.
#[derive(Clone, PartialEq)]
struct Target {
    range: Range<usize>,
    to: SharedString,
}

#[derive(Default)]
struct TableState {
    aligns: Vec<Alignment>,
    head: bool,
    row: usize,
    col: usize,
}

impl TableState {
    fn cell_align(&self) -> Option<Alignment> {
        if self.aligns.is_empty() {
            None
        } else if self.head {
            Some(Alignment::Center)
        } else {
            self.aligns.get(self.col).copied()
        }
    }
}

struct Builder<'a> {
    look: &'a Look,
    parsed: &'a Parsed,
    end: usize,
    blocks: Vec<Holder>,
    draft: Draft,
    lines: Vec<Rc<Line>>,
    links: Vec<Target>,
    notes: Vec<Target>,
    /// The source end of the last text put in the draft.
    source_at: usize,
    styles: Vec<TextStyleRefinement>,
    link_depth: usize,
    /// The next number of each open list; none for a bullet list.
    lists: Vec<Option<u64>>,
    table: TableState,
    in_comment: bool,
    notes_rule: bool,
    selection: Option<Range<usize>>,
    scrolls: &'a mut BTreeMap<usize, ScrollHandle>,
    used_scrolls: BTreeSet<usize>,
}

/// What a frame drew: the blocks, and the lines of text in them for the mouse.
struct Built {
    element: AnyElement,
    rendered: Rendered,
}

impl<'a> Builder<'a> {
    fn build(
        look: &'a Look,
        parsed: &'a Parsed,
        selection: Option<Range<usize>>,
        scrolls: &'a mut BTreeMap<usize, ScrollHandle>,
        window: &mut Window,
        cx: &mut App,
    ) -> Built {
        let mut root = div();
        root.style().text.font_size = look.root_size;
        let mut builder = Builder {
            look,
            parsed,
            end: parsed.end(),
            blocks: vec![root.into()],
            draft: Draft::default(),
            lines: Vec::new(),
            links: Vec::new(),
            notes: Vec::new(),
            source_at: 0,
            styles: Vec::new(),
            link_depth: 0,
            lists: Vec::new(),
            table: TableState::default(),
            in_comment: false,
            notes_rule: false,
            selection,
            scrolls,
            used_scrolls: BTreeSet::new(),
        };
        for (index, (range, piece)) in parsed.pieces.iter().enumerate() {
            builder.step(index, range, piece, window, cx);
        }
        builder.flush();
        let used = std::mem::take(&mut builder.used_scrolls);
        builder.scrolls.retain(|start, _| used.contains(start));
        let element = builder
            .blocks
            .drain(..)
            .next()
            .map_or_else(|| div().into_any_element(), Holder::into_any);
        Built {
            element,
            rendered: Rendered {
                lines: builder.lines.into(),
                links: builder.links.into(),
                notes: builder.notes.into(),
            },
        }
    }

    fn step(
        &mut self,
        index: usize,
        range: &Range<usize>,
        piece: &Piece,
        window: &mut Window,
        cx: &mut App,
    ) {
        let parsed = self.parsed;
        match piece {
            Piece::Start(tag) => self.start(tag, range, index, window, cx),
            Piece::End(end) => self.finish(end),
            Piece::Text { shown, .. } => self.text(shown, range.clone()),
            Piece::Code(shown) => self.code_span(shown, range.clone()),
            Piece::Html => {
                let html = parsed.source.get(range.clone()).unwrap_or_default();
                if html.starts_with("<!--") {
                    self.in_comment = true;
                }
                if html.trim_end().ends_with("-->") {
                    self.in_comment = false;
                } else if !self.in_comment {
                    self.text(html, range.clone());
                }
            }
            Piece::InlineHtml => {
                let html = parsed.source.get(range.clone()).unwrap_or_default();
                if let Some(code) = html
                    .strip_prefix("<code>")
                    .and_then(|html| html.strip_suffix("</code>"))
                {
                    let start = range.start + "<code>".len();
                    self.code_span(code, start..start + code.len());
                } else if html.starts_with("<code>") {
                    self.styles.push(self.look.inline_code.clone());
                } else if html.trim_end().starts_with("</code>") {
                    self.styles.pop();
                } else {
                    self.text(html, range.clone());
                }
            }
            Piece::FootnoteRef(label) => {
                self.notes.push(Target {
                    range: range.clone(),
                    to: label.clone(),
                });
                self.styles.push(self.look.link.clone());
                self.text(&format!("[{label}]"), range.clone());
                self.styles.pop();
            }
            Piece::SoftBreak => self.text(" ", range.clone()),
            Piece::HardBreak => self.text("\n", range.clone()),
            Piece::Rule => {
                let rule = div()
                    .border_b_1()
                    .my(self.look.paragraph_spacing)
                    .border_color(self.look.border);
                self.open(rule, range);
                self.close();
            }
            Piece::TaskMarker(_) => {}
        }
    }

    fn start(
        &mut self,
        tag: &Tag<'static>,
        range: &Range<usize>,
        index: usize,
        window: &mut Window,
        cx: &mut App,
    ) {
        let look = self.look;
        match tag {
            Tag::Paragraph => {
                let align = self.block_align();
                let paragraph = div()
                    .mb(look.paragraph_spacing)
                    .line_height(look.paragraph_line_height);
                self.styles.push(TextStyleRefinement {
                    text_align: Some(align),
                    ..Default::default()
                });
                self.open(aligned(paragraph, align), range);
            }
            Tag::Heading { level, .. } => {
                let align = self.block_align();
                let index = level_index(*level);
                let level_style = look.levels[index].clone();
                let mut heading = div();
                heading.style().text = level_style.clone();
                heading = match index {
                    0 => heading
                        .pb_2()
                        .border_b_1()
                        .border_color(look.border_variant),
                    1 => heading
                        .pb_1()
                        .border_b_1()
                        .border_color(look.border_variant),
                    _ => heading,
                };
                let mut heading = aligned(heading, align);
                heading.style().refine(&look.heading);
                let mut text = look.heading.text.clone();
                text.refine(&level_style);
                self.styles.push(TextStyleRefinement {
                    text_align: Some(align),
                    ..text
                });
                self.open(heading, range);
            }
            Tag::BlockQuote(kind) => {
                let color = kind.map_or(look.border, |kind| alert_color(look, kind));
                let mut quote = div()
                    .pl_4()
                    .mb(look.paragraph_spacing)
                    .border_l_4()
                    .border_color(color);
                if let Some(kind) = kind {
                    quote = quote.child(alert_header(*kind, color));
                }
                self.styles.push(look.quote.clone());
                self.open(quote, range);
            }
            Tag::CodeBlock(_) => {
                let scroll = self.scrolls.entry(range.start).or_default().clone();
                self.used_scrolls.insert(range.start);
                let bar = Scrollbars::new(ScrollAxes::Horizontal)
                    .id(("doc-code-scrollbar", range.start))
                    .tracked_scroll_handle(&scroll)
                    .with_track_along(ScrollAxes::Horizontal, look.code_background)
                    .notify_content();
                let mut frame = div()
                    .group("code_block")
                    .relative()
                    .w_full()
                    .rounded_lg()
                    .custom_scrollbars(bar, window, cx)
                    .rounded_md()
                    .border_1()
                    .border_color(look.border_variant);
                frame.style().refine(&look.code_block);
                self.open(frame, range);
                let body = div()
                    .id(("doc-code", range.start))
                    .rounded_lg()
                    .flex()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll);
                self.styles.push(look.code_block.text.clone());
                self.open(body, range);
            }
            Tag::HtmlBlock => self.open(div(), range),
            Tag::List(first) => {
                self.lists.push(*first);
                let top = self.lists.len() == 1;
                let list = div().pl_2p5().when(top, |list| list.mb(look.list_spacing));
                self.open(list, range);
            }
            Tag::Item => {
                let bullet = self.bullet(range, index, window);
                let item = div()
                    .mb_1()
                    .gap_1()
                    .line_height(look.paragraph_line_height)
                    .h_flex()
                    .items_start()
                    .child(bullet);
                self.open(item, range);
                // Without `w_0` the text does not wrap to the width of the item.
                self.open(div().flex_1().w_0(), range);
            }
            Tag::Emphasis => self.styles.push(TextStyleRefinement {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            }),
            Tag::Strong => self.styles.push(TextStyleRefinement {
                font_weight: Some(FontWeight::BOLD),
                color: Some(look.text),
                ..Default::default()
            }),
            Tag::Strikethrough => self.styles.push(TextStyleRefinement {
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.),
                    color: None,
                }),
                ..Default::default()
            }),
            Tag::Link { dest_url, .. } => {
                self.link_depth += 1;
                self.links.push(Target {
                    range: range.clone(),
                    to: dest_url.to_string().into(),
                });
                self.styles.push(look.link.clone());
            }
            Tag::FootnoteDefinition(label) => {
                if !self.notes_rule {
                    self.notes_rule = true;
                    let rule = div().border_t_1().mt_2().border_color(look.border);
                    self.open(rule, range);
                    self.close();
                }
                let note = div()
                    .pt_1()
                    .mb_1()
                    .line_height(rems(1.3))
                    .text_size(rems(0.85))
                    .h_flex()
                    .items_start()
                    .gap_2()
                    .child(div().text_size(rems(0.85)).child(format!("{label}.")));
                self.open(note, range);
                self.open(div().flex_1().w_0(), range);
            }
            Tag::Table(aligns) => {
                self.table = TableState {
                    aligns: aligns.clone(),
                    ..Default::default()
                };
                let columns = u16::try_from(aligns.len()).unwrap_or(u16::MAX);
                self.open(div().flex(), range);
                let bar = Scrollbars::new(ScrollAxes::Horizontal)
                    .id(("doc-table-scrollbar", range.start))
                    .notify_content();
                let grid = div()
                    .id(("doc-table", range.start))
                    .min_w_0()
                    .grid()
                    .grid_cols_max_content(columns)
                    .mb_2()
                    .border(px(1.5))
                    .border_color(look.border)
                    .rounded_sm()
                    .restrict_scroll_to_axis()
                    .custom_scrollbars(bar, window, cx);
                self.open(grid, range);
            }
            Tag::TableHead => {
                self.table.head = true;
                self.styles.push(TextStyleRefinement {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                });
            }
            Tag::TableRow => self.table.col = 0,
            Tag::TableCell => {
                let (head, row, col) = (self.table.head, self.table.row, self.table.col);
                let alignment = self.table.cell_align();
                let align = alignment
                    .and_then(text_align_of)
                    .unwrap_or(look.base.text_align);
                let mut cell = div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .when(col > 0, |cell| cell.border_l_1())
                    .when(row > 0, |cell| cell.border_t_1())
                    .border_color(look.border)
                    .px(look.cell_padding.x)
                    .py(look.cell_padding.y)
                    .when(head, |cell| cell.bg(look.head_background))
                    .when(!head && row % 2 == 1, |cell| {
                        cell.bg(look.stripe_background)
                    });
                cell = match alignment {
                    Some(Alignment::Center) => cell.items_center(),
                    Some(Alignment::Right) => cell.items_end(),
                    _ => cell,
                };
                self.styles.push(TextStyleRefinement {
                    text_align: Some(align),
                    ..Default::default()
                });
                self.open(cell, range);
                let inner = div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .justify_center()
                    .text_align(align);
                self.open(inner, range);
            }
            Tag::Image { .. }
            | Tag::Superscript
            | Tag::Subscript
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn finish(&mut self, end: &TagEnd) {
        match end {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::BlockQuote(_) => {
                self.close();
                self.styles.pop();
            }
            TagEnd::CodeBlock => {
                self.trim_newline();
                self.close();
                self.styles.pop();
                self.close();
            }
            TagEnd::HtmlBlock => self.close(),
            TagEnd::List(_) => {
                self.lists.pop();
                self.close();
            }
            TagEnd::Item | TagEnd::FootnoteDefinition => {
                self.close();
                self.close();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.link_depth = self.link_depth.saturating_sub(1);
                self.styles.pop();
            }
            TagEnd::Table => {
                self.close();
                self.close();
                self.table = TableState::default();
            }
            TagEnd::TableHead => {
                self.styles.pop();
                self.table.head = false;
            }
            TagEnd::TableRow => self.table.row += 1,
            TagEnd::TableCell => {
                self.cell_checkbox();
                self.close();
                self.close();
                self.styles.pop();
                self.table.col += 1;
            }
            TagEnd::Image
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::MetadataBlock(_) => {}
        }
    }

    /// The text style of the text now put in the draft.
    fn style(&self) -> TextStyle {
        let mut style = self.look.base.clone();
        for refinement in &self.styles {
            style.refine(refinement);
        }
        style
    }

    /// A paragraph or heading in a table cell takes the column's alignment.
    fn block_align(&self) -> TextAlign {
        self.table
            .cell_align()
            .and_then(text_align_of)
            .unwrap_or(self.look.base.text_align)
    }

    fn open(&mut self, block: impl Into<Holder>, range: &Range<usize>) {
        let mut block = block.into();
        self.flush();
        // The doc's first block has no top margin, and its last no bottom margin.
        if range.start == 0 {
            block.style().refine(&StyleRefinement {
                margin: EdgesRefinement {
                    top: Some(Length::Definite(px(0.).into())),
                    ..Default::default()
                },
                ..Default::default()
            });
        }
        if range.end == self.end {
            block.style().refine(&StyleRefinement {
                margin: EdgesRefinement {
                    bottom: Some(Length::Definite(rems(0.).into())),
                    ..Default::default()
                },
                ..Default::default()
            });
        }
        self.blocks.push(block);
    }

    fn close(&mut self) {
        self.flush();
        if self.blocks.len() < 2 {
            return;
        }
        if let Some(block) = self.blocks.pop() {
            self.append(block.into_any());
        }
    }

    fn append(&mut self, child: AnyElement) {
        if let Some(block) = self.blocks.last_mut() {
            block.push(child);
        }
    }

    fn text(&mut self, shown: &str, range: Range<usize>) {
        self.draft.map.push(Mapping {
            shown: self.draft.text.len(),
            source: range.start,
        });
        self.draft.text.push_str(shown);
        self.source_at = range.end;
        let run = self.style().to_run(shown.len());
        self.draft.runs.push(run);
    }

    /// Inline code: the code font, on a chip painted behind the glyphs.
    fn code_span(&mut self, shown: &str, range: Range<usize>) {
        let mut style = self.look.inline_code.clone();
        let chip = style.background_color.take();
        if self.link_depth > 0 {
            style.color = self.look.link.color.or(style.color);
        }
        self.styles.push(style);
        let start = self.draft.text.len();
        self.text(shown, range);
        let end = self.draft.text.len();
        if let Some(color) = chip
            && start < end
        {
            self.draft.chips.push((start..end, color));
        }
        self.styles.pop();
    }

    /// A code block's last newline is not drawn as an empty row.
    fn trim_newline(&mut self) {
        if self.draft.text.ends_with('\n') {
            self.draft.text.pop();
            if let Some(run) = self.draft.runs.last_mut() {
                run.len = run.len.saturating_sub(1);
            }
            self.source_at = self.source_at.saturating_sub(1);
        }
    }

    fn flush(&mut self) {
        let align = self.style().text_align;
        let draft = std::mem::take(&mut self.draft);
        if draft.text.is_empty() {
            return;
        }
        let marks = match (&self.selection, draft.map.first()) {
            (Some(selection), Some(first)) => {
                let start = selection.start.max(first.source);
                let end = selection.end.min(self.source_at);
                if start < end {
                    vec![(start..end, self.look.selection)]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        let text = StyledText::new(draft.text).with_runs(draft.runs);
        let line = Rc::new(Line {
            layout: text.layout().clone(),
            shown: Cell::new(None),
            map: draft.map,
            end: self.source_at,
            align,
            marks,
            chips: draft.chips,
        });
        self.lines.push(line.clone());
        self.append(LineElement { text, line }.into_any_element());
    }

    fn bullet(&mut self, range: &Range<usize>, index: usize, window: &Window) -> AnyElement {
        if let Some(done) = self.task_marker(index) {
            let parsed = self.parsed;
            let source = parsed.source.get(range.clone()).unwrap_or_default();
            let checkbox = Checkbox::new(
                ElementId::Name(source.to_string().into()),
                ToggleState::from(done),
            )
            .fill()
            .visualization_only(true);
            let height = self
                .look
                .paragraph_line_height
                .to_pixels(self.style().font_size, window.rem_size());
            // A checkbox is taller than a text bullet: centre it in a slot one line high, so it
            // sits by the first line.
            return div()
                .h(height)
                .flex()
                .items_center()
                .child(checkbox)
                .into_any_element();
        }
        let number = self.lists.last_mut().and_then(|next| {
            let next = next.as_mut()?;
            *next += 1;
            Some(*next - 1)
        });
        match number {
            Some(number) => div().child(format!("{number}.")).into_any_element(),
            None => div().child("•").into_any_element(),
        }
    }

    /// The task marker of the item at `index`. In a loose list it follows the paragraph start.
    fn task_marker(&self, index: usize) -> Option<bool> {
        let pieces = &self.parsed.pieces;
        let mut at = index + 1;
        if matches!(pieces.get(at), Some((_, Piece::Start(Tag::Paragraph)))) {
            at += 1;
        }
        match pieces.get(at) {
            Some((_, Piece::TaskMarker(done))) => Some(*done),
            _ => None,
        }
    }

    /// A table cell that holds only `[x]`, `[X]` or `[ ]` shows a checkbox instead.
    fn cell_checkbox(&mut self) {
        let text = &self.draft.text;
        let trimmed = text.trim();
        if !matches!(trimmed, "[x]" | "[X]" | "[ ]") {
            return;
        }
        let done = trimmed != "[ ]";
        let lead = text.len() - text.trim_start().len();
        let Some(marker) = source_range(&self.draft.map, lead..lead + trimmed.len()) else {
            return;
        };
        self.draft = Draft::default();
        let checkbox = Checkbox::new(
            ElementId::Name(format!("doc-table-check-{}-{}", marker.start, marker.end).into()),
            ToggleState::from(done),
        )
        .fill()
        .visualization_only(true);
        let row = h_flex().w_full();
        let row = match self.style().text_align {
            TextAlign::Left => row.justify_start(),
            TextAlign::Center => row.justify_center(),
            TextAlign::Right => row.justify_end(),
        };
        self.append(row.child(checkbox).into_any_element());
    }
}

fn source_range(map: &[Mapping], shown: Range<usize>) -> Option<Range<usize>> {
    if shown.start >= shown.end {
        return None;
    }
    let at = |index: usize| {
        map.iter()
            .take_while(|mapping| mapping.shown <= index)
            .last()
            .map(|mapping| mapping.source + (index - mapping.shown))
    };
    Some(at(shown.start)?..at(shown.end - 1)? + 1)
}

// The lines of text drawn in a frame, and how a point or a source byte maps onto them.

struct Line {
    layout: TextLayout,
    /// Where it was drawn, clipped; none until it is drawn.
    shown: Cell<Option<Bounds<Pixels>>>,
    map: Vec<Mapping>,
    /// The source end of its last text.
    end: usize,
    align: TextAlign,
    /// Selected source ranges, painted behind the glyphs.
    marks: Vec<(Range<usize>, Hsla)>,
    /// Inline code, in shown bytes.
    chips: Vec<(Range<usize>, Hsla)>,
}

/// A text line between hard breaks, as laid out, with its wrap rows.
struct Band {
    start: usize,
    end: usize,
    top: Pixels,
    layout: Arc<WrappedLineLayout>,
}

impl Line {
    fn drawn(&self) -> bool {
        self.shown.get().is_some()
    }

    fn first_source(&self) -> usize {
        self.map.first().map_or(self.end, |mapping| mapping.source)
    }

    fn bands(&self) -> Vec<Band> {
        let height = self.layout.line_height();
        let mut top = self.layout.bounds().top();
        let mut start = 0;
        let mut bands = Vec::new();
        for layout in self.layout.line_layouts() {
            let end = start + layout.len();
            let below = top + layout.size(height).height;
            bands.push(Band {
                start,
                end,
                top,
                layout,
            });
            top = below;
            start = end + 1;
        }
        bands
    }

    /// The x offset of a row of `start_x..end_x` under the line's alignment.
    fn align_offset(&self, width: Pixels, start_x: Pixels, end_x: Pixels) -> Pixels {
        let row = end_x - start_x;
        match self.align {
            TextAlign::Left => px(0.),
            TextAlign::Center => ((width - row) / 2.).max(px(0.)),
            TextAlign::Right => (width - row).max(px(0.)),
        }
    }

    /// Calls `each` with the box of every row piece of a shown range.
    fn boxes(&self, bands: &[Band], shown: Range<usize>, mut each: impl FnMut(Bounds<Pixels>)) {
        let bounds = self.layout.bounds();
        let height = self.layout.line_height();
        for band in bands {
            if band.start >= shown.end {
                break;
            }
            if band.end <= shown.start {
                continue;
            }
            let unwrapped = &band.layout.unwrapped_layout;
            let row_ends = band
                .layout
                .wrap_boundaries()
                .iter()
                .filter_map(|wrap| unwrapped.runs.get(wrap.run_ix)?.glyphs.get(wrap.glyph_ix))
                .map(|glyph| (band.start + glyph.index, glyph.position.x))
                .chain([(band.end, unwrapped.width)]);
            let (mut row_start, mut row_x, mut top) = (band.start, px(0.), band.top);
            for (row_end, end_x) in row_ends {
                let from = shown.start.max(row_start);
                let to = shown.end.min(row_end);
                if from < to {
                    let offset = self.align_offset(bounds.size.width, row_x, end_x);
                    let x = |index: usize| {
                        bounds.left() + offset + unwrapped.x_for_index(index - band.start) - row_x
                    };
                    each(Bounds::from_corners(
                        point(x(from), top),
                        point(x(to), top + height),
                    ));
                }
                row_start = row_end;
                row_x = end_x;
                top += height;
            }
        }
    }

    fn source_boxes(&self, bands: &[Band], source: Range<usize>, each: impl FnMut(Bounds<Pixels>)) {
        if source.start >= source.end {
            return;
        }
        let shown = self.shown_index(source.start)..self.shown_index(source.end);
        self.boxes(bands, shown, each);
    }

    fn paint_chips(&self, window: &mut Window) {
        if self.chips.is_empty() {
            return;
        }
        let bands = self.bands();
        for (shown, color) in &self.chips {
            self.boxes(&bands, shown.clone(), |bounds| {
                // One px out at the sides; in by a tenth of the row at the top and bottom, so
                // the chip hugs the glyphs.
                let outset = px(1.);
                let inset = bounds.size.height * 0.1;
                let chip = Bounds {
                    origin: point(bounds.origin.x - outset, bounds.origin.y + inset),
                    size: size(
                        bounds.size.width + outset * 2.,
                        bounds.size.height - inset * 2.,
                    ),
                };
                window.paint_quad(quad(
                    chip,
                    px(4.),
                    *color,
                    Edges::default(),
                    Hsla::transparent_black(),
                    BorderStyle::default(),
                ));
            });
        }
    }

    fn paint_marks(&self, window: &mut Window) {
        if self.marks.is_empty() {
            return;
        }
        let bands = self.bands();
        for (source, color) in &self.marks {
            self.source_boxes(&bands, source.clone(), |bounds| {
                window.paint_quad(quad(
                    bounds,
                    px(0.),
                    *color,
                    Edges::default(),
                    Hsla::transparent_black(),
                    BorderStyle::default(),
                ));
            });
        }
    }

    fn mapping_for(&self, found: Result<usize, usize>) -> Option<&Mapping> {
        let at = match found {
            Ok(at) => at,
            Err(at) => at.checked_sub(1)?,
        };
        self.map.get(at)
    }

    fn shown_index(&self, source: usize) -> usize {
        let len = self.layout.len();
        if source >= self.end {
            return len;
        }
        let found = self
            .map
            .binary_search_by_key(&source, |mapping| mapping.source);
        let Some(mapping) = self.mapping_for(found) else {
            return 0;
        };
        (mapping.shown + (source - mapping.source)).min(len)
    }

    fn source_index(&self, shown: usize) -> usize {
        if shown >= self.layout.len() {
            return self.end;
        }
        let found = self
            .map
            .binary_search_by_key(&shown, |mapping| mapping.shown);
        let Some(mapping) = self.mapping_for(found) else {
            return self.first_source();
        };
        mapping.source + (shown - mapping.shown)
    }

    /// The source end for a shown end. At the start of a part that follows hidden source (such
    /// as the backticks of inline code) it is the end of the part before.
    fn source_end(&self, shown: usize) -> usize {
        if shown >= self.layout.len() {
            return self.end;
        }
        match self
            .map
            .binary_search_by_key(&shown, |mapping| mapping.shown)
        {
            Err(at) => match at.checked_sub(1).and_then(|at| self.map.get(at)) {
                Some(mapping) => mapping.source + (shown - mapping.shown),
                None => self.first_source(),
            },
            Ok(at) => {
                if let Some(before) = at.checked_sub(1).and_then(|at| self.map.get(at))
                    && let Some(mapping) = self.map.get(at)
                {
                    let before_end = before.source + (mapping.shown - before.shown);
                    if before_end < mapping.source {
                        return before_end;
                    }
                }
                self.map.get(at).map_or(self.end, |mapping| mapping.source)
            }
        }
    }

    fn source_at(&self, position: Point<Pixels>) -> Result<usize, usize> {
        let position = self.unaligned(position).unwrap_or(position);
        match self.layout.index_for_position(position) {
            Ok(shown) => Ok(self.source_index(shown)),
            Err(shown) => Err(self.source_index(shown)),
        }
    }

    /// A point moved back by the alignment offset of its row, for a centred or right line.
    fn unaligned(&self, position: Point<Pixels>) -> Option<Point<Pixels>> {
        if self.align == TextAlign::Left {
            return None;
        }
        let wrapped = self.layout.line_layout_for_index(0)?;
        let bounds = self.layout.bounds();
        let height = self.layout.line_height();
        let row = ((position.y - bounds.top()).max(px(0.)) / height) as usize;
        let wraps = wrapped.wrap_boundaries();
        let x_of = |at: usize| {
            wraps.get(at).and_then(|wrap| {
                let glyph = wrapped
                    .unwrapped_layout
                    .runs
                    .get(wrap.run_ix)?
                    .glyphs
                    .get(wrap.glyph_ix)?;
                Some(glyph.position.x)
            })
        };
        let start_x = match row.checked_sub(1) {
            Some(before) => x_of(before).unwrap_or(px(0.)),
            None => px(0.),
        };
        let end_x = x_of(row).unwrap_or(wrapped.unwrapped_layout.width);
        let offset = self.align_offset(bounds.size.width, start_x, end_x);
        Some(point(position.x - offset, position.y))
    }
}

/// The lines, links and footnote references of one frame.
#[derive(Clone, Default)]
struct Rendered {
    lines: Rc<[Rc<Line>]>,
    links: Rc<[Target]>,
    notes: Rc<[Target]>,
}

/// Word, punctuation or space, for a double click. Word wins over the others.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharKind {
    Space,
    Punctuation,
    Word,
}

fn char_kind(c: char) -> CharKind {
    if c.is_alphanumeric() || c == '_' {
        CharKind::Word
    } else if c.is_whitespace() {
        CharKind::Space
    } else {
        CharKind::Punctuation
    }
}

impl Rendered {
    fn drawn(&self) -> impl Iterator<Item = &Rc<Line>> {
        self.lines.iter().filter(|line| line.drawn())
    }

    fn last_end(&self) -> usize {
        self.lines.last().map_or(0, |line| line.end)
    }

    fn source_at(&self, position: Point<Pixels>) -> Result<usize, usize> {
        let mut fallback: Option<&Rc<Line>> = None;
        let mut lines = self.drawn().peekable();
        while let Some(line) = lines.next() {
            let bounds = line.layout.bounds();
            // Inside: table columns can overlap in y, so check the box first.
            if bounds.contains(&position) {
                return line.source_at(position);
            }
            if position.y <= bounds.bottom() && fallback.is_none() {
                fallback = Some(line);
            }
            if position.y > bounds.bottom()
                && let Some(next) = lines.peek()
                && position.y < next.layout.bounds().top()
            {
                return Err(line.end);
            }
        }
        match fallback {
            Some(line) => line.source_at(position),
            None => Err(self.last_end()),
        }
    }

    fn source_at_visible(&self, position: Point<Pixels>) -> Option<usize> {
        self.drawn().find_map(|line| {
            if line.shown.get()?.contains(&position) {
                line.source_at(position).ok()
            } else {
                None
            }
        })
    }

    fn position_of(&self, source: usize) -> Option<(Point<Pixels>, Pixels)> {
        let line = self.drawn().find(|line| source <= line.end)?;
        let source = source.max(line.first_source());
        let shown = line.shown_index(source);
        let position = line.layout.position_for_index(shown)?;
        Some((position, line.layout.line_height()))
    }

    fn word_at(&self, source: usize) -> Range<usize> {
        let Some(line) = self.drawn().find(|line| source <= line.end) else {
            return source..source;
        };
        let first = line.map.first().map_or(0, |mapping| mapping.shown);
        let at = line.shown_index(source).saturating_sub(first);
        let text = line.layout.text();
        let (Some(before), Some(after)) = (text.get(..at), text.get(at..)) else {
            return source..source;
        };
        let kind = std::cmp::max(
            before.chars().next_back().map(char_kind),
            after.chars().next().map(char_kind),
        );
        let start = at
            - before
                .chars()
                .rev()
                .take_while(|c| Some(char_kind(*c)) == kind)
                .map(char::len_utf8)
                .sum::<usize>();
        let end = at
            + after
                .chars()
                .take_while(|c| Some(char_kind(*c)) == kind)
                .map(char::len_utf8)
                .sum::<usize>();
        line.source_index(first + start)..line.source_end(first + end)
    }

    fn line_at(&self, source: usize) -> Range<usize> {
        match self.drawn().find(|line| source <= line.end) {
            Some(line) => line.first_source()..line.end,
            None => source..source,
        }
    }

    fn text_of(&self, range: Range<usize>) -> String {
        let mut parts = Vec::new();
        for line in self.drawn() {
            if range.start > line.end {
                continue;
            }
            if range.end < line.first_source() {
                break;
            }
            let text = line.layout.text();
            let start = if range.start < line.first_source() {
                0
            } else {
                line.shown_index(range.start)
            };
            let end = line.shown_index(range.end.min(line.end)).min(text.len());
            parts.push(text.get(start..end).unwrap_or_default().to_owned());
        }
        parts.join("\n")
    }

    fn boxes_of(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let mut boxes = Vec::new();
        for line in self.drawn() {
            if range.end <= line.first_source() {
                break;
            }
            if range.start >= line.end {
                continue;
            }
            let bands = line.bands();
            let source = range.start.max(line.first_source())..range.end.min(line.end);
            line.source_boxes(&bands, source, |bounds| boxes.push(bounds));
        }
        boxes
    }

    fn link_at(&self, source: usize) -> Option<&Target> {
        self.links.iter().find(|link| link.range.contains(&source))
    }

    fn note_at(&self, source: usize) -> Option<&Target> {
        self.notes.iter().find(|note| note.range.contains(&source))
    }

    /// All shown text, a newline between lines, for the platform's text queries.
    fn all_text(&self) -> String {
        self.drawn()
            .map(|line| line.layout.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn utf16_of(&self, source: usize) -> usize {
        self.text_of(0..source).encode_utf16().count()
    }

    fn source_of_utf16(&self, utf16: usize) -> usize {
        let lines: Vec<&Rc<Line>> = self.drawn().collect();
        let mut start = 0;
        for (index, line) in lines.iter().enumerate() {
            let text = line.layout.text();
            let end = start + text.encode_utf16().count();
            if utf16 <= end {
                let shown = byte_of_utf16(&text, utf16.saturating_sub(start), false);
                return line.source_index(shown);
            }
            start = end + usize::from(index + 1 < lines.len());
        }
        lines.last().map_or(0, |line| line.end)
    }
}

/// The byte offset of a UTF-16 offset, moved off the middle of a character: back, or on to its
/// end with `forward`.
fn byte_of_utf16(text: &str, utf16: usize, forward: bool) -> usize {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        let next = units + c.len_utf16();
        if utf16 < next {
            return if forward && utf16 > units {
                at + c.len_utf8()
            } else {
                at
            };
        }
        units = next;
    }
    text.len()
}

// State and input.

#[derive(Clone, Default)]
enum SelectMode {
    #[default]
    Character,
    Word(Range<usize>),
    Line(Range<usize>),
    All,
}

/// A selection in source bytes.
#[derive(Clone, Default)]
struct Selection {
    start: usize,
    end: usize,
    reversed: bool,
    /// The mouse is still down.
    pending: bool,
    mode: SelectMode,
}

impl Selection {
    fn tail(&self) -> usize {
        if self.reversed { self.end } else { self.start }
    }

    fn set_head(&mut self, head: usize, rendered: &Rendered) {
        match &self.mode {
            SelectMode::Character => {
                if head < self.tail() {
                    if !self.reversed {
                        self.end = self.start;
                        self.reversed = true;
                    }
                    self.start = head;
                } else {
                    if self.reversed {
                        self.start = self.end;
                        self.reversed = false;
                    }
                    self.end = head;
                }
            }
            SelectMode::Word(first) | SelectMode::Line(first) => {
                let around = if matches!(self.mode, SelectMode::Word(_)) {
                    rendered.word_at(head)
                } else {
                    rendered.line_at(head)
                };
                let first = first.clone();
                if head < first.start {
                    (self.start, self.end, self.reversed) = (around.start, first.end, true);
                } else if head >= first.end {
                    (self.start, self.end, self.reversed) = (first.start, around.end, false);
                } else {
                    (self.start, self.end, self.reversed) = (first.start, first.end, false);
                }
            }
            SelectMode::All => {
                (self.start, self.end, self.reversed) = (0, rendered.last_end(), false);
            }
        }
    }
}

struct DocState {
    parsed: Rc<Parsed>,
    selection: Selection,
    pressed_link: Option<Target>,
    pressed_note: Option<Target>,
    /// A source byte to scroll into view at the next frame.
    autoscroll: Option<usize>,
    focus: FocusHandle,
    /// The horizontal scroll of each code block, by its source start.
    code_scrolls: BTreeMap<usize, ScrollHandle>,
}

impl DocState {
    fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        phase: DispatchPhase,
        hitbox: &Hitbox,
        rendered: &Rendered,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !hitbox.is_hovered(window) {
            if phase.capture() && event.button == MouseButton::Left {
                self.selection = Selection::default();
                self.pressed_link = None;
                cx.notify();
            }
            return;
        }
        if !phase.bubble() || event.button == MouseButton::Right {
            return;
        }
        let found = rendered.source_at(event.position);
        if let Ok(source) = found {
            if let Some(note) = rendered.note_at(source) {
                self.pressed_note = Some(note.clone());
            } else if let Some(link) = rendered.link_at(source) {
                self.pressed_link = Some(link.clone());
            }
        }
        if self.pressed_note.is_none() && self.pressed_link.is_none() {
            let (Ok(source) | Err(source)) = found;
            let (range, mode, reversed) = match event.click_count {
                1 if event.modifiers.shift => {
                    let tail = self.selection.tail();
                    if source < tail {
                        (source..tail, SelectMode::Character, true)
                    } else {
                        (tail..source, SelectMode::Character, false)
                    }
                }
                1 => (source..source, SelectMode::Character, false),
                2 => {
                    let word = rendered.word_at(source);
                    (word.clone(), SelectMode::Word(word), false)
                }
                3 => {
                    let line = rendered.line_at(source);
                    (line.clone(), SelectMode::Line(line), false)
                }
                _ => (0..rendered.last_end(), SelectMode::All, false),
            };
            self.selection = Selection {
                start: range.start,
                end: range.end,
                reversed,
                pending: true,
                mode,
            };
            window.focus(&self.focus, cx);
        }
        window.prevent_default();
        cx.notify();
    }

    fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        phase: DispatchPhase,
        hovered: bool,
        rendered: &Rendered,
        was_on_target: bool,
        cx: &mut Context<Self>,
    ) {
        if phase.capture() {
            return;
        }
        if self.selection.pending {
            let (Ok(source) | Err(source)) = rendered.source_at(event.position);
            self.selection.set_head(source, rendered);
            self.scroll_code_block(source, event.position);
            self.autoscroll = Some(source);
            cx.notify();
        } else {
            let on_target = hovered
                && rendered
                    .source_at(event.position)
                    .ok()
                    .is_some_and(|source| on_target(rendered, source));
            if on_target != was_on_target {
                cx.notify();
            }
        }
    }

    fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        phase: DispatchPhase,
        rendered: &Rendered,
        cx: &mut Context<Self>,
    ) {
        if phase.bubble() {
            let source = rendered.source_at(event.position).ok();
            if let Some(note) = self.pressed_note.take()
                && source.and_then(|source| rendered.note_at(source)) == Some(&note)
            {
                if let Some(start) = self.parsed.note_starts.get(&note.to) {
                    self.autoscroll = Some(*start);
                    cx.notify();
                }
            } else if let Some(link) = self.pressed_link.take()
                && source.and_then(|source| rendered.link_at(source)) == Some(&link)
            {
                open_link(&link.to, cx);
            }
        } else if self.selection.pending {
            self.selection.pending = false;
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            {
                let range = self.selection.start..self.selection.end;
                cx.write_to_primary(gpui::ClipboardItem::new_string(rendered.text_of(range)));
            }
            cx.notify();
        }
    }

    /// While selecting, a point left or right of a code block scrolls it that way.
    fn scroll_code_block(&self, source: usize, position: Point<Pixels>) {
        let Some((_, scroll)) = self.code_scrolls.range(..=source).next_back() else {
            return;
        };
        let bounds = scroll.bounds();
        if position.y < bounds.top() || position.y > bounds.bottom() {
            return;
        }
        let delta = if position.x < bounds.left() {
            bounds.left() - position.x
        } else if position.x > bounds.right() {
            bounds.right() - position.x
        } else {
            return;
        };
        let offset = scroll.offset();
        scroll.set_offset(point(offset.x + delta, offset.y));
    }
}

fn on_target(rendered: &Rendered, source: usize) -> bool {
    rendered.link_at(source).is_some() || rendered.note_at(source).is_some()
}

/// The doc for the platform's text queries (look up, services). It takes no typing.
struct DocInput {
    state: Entity<DocState>,
    rendered: Rendered,
}

impl InputHandler for DocInput {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        let selection = &self.state.read(cx).selection;
        Some(UTF16Selection {
            range: self.rendered.utf16_of(selection.start)..self.rendered.utf16_of(selection.end),
            reversed: selection.reversed,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        None
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        if range_utf16.start > range_utf16.end {
            return None;
        }
        let text = self.rendered.all_text();
        let start = byte_of_utf16(&text, range_utf16.start, false);
        let end = byte_of_utf16(&text, range_utf16.end, true);
        let utf16 = |byte: usize| {
            text.get(..byte)
                .map_or(0, |head| head.encode_utf16().count())
        };
        let clipped = utf16(start)..utf16(end);
        if clipped != range_utf16 {
            *adjusted_range = Some(clipped);
        }
        Some(text.get(start..end).unwrap_or_default().to_owned())
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        _text: &str,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        _text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {}

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let source = self.rendered.source_of_utf16(range_utf16.start)
            ..self.rendered.source_of_utf16(range_utf16.end);
        self.rendered
            .boxes_of(source.clone())
            .into_iter()
            .next()
            .or_else(|| {
                let (origin, height) = self.rendered.position_of(source.start)?;
                Some(Bounds {
                    origin,
                    size: size(px(0.), height),
                })
            })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        let source = self.rendered.source_at_visible(point)?;
        Some(self.rendered.utf16_of(source))
    }

    fn accepts_text_input(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        false
    }
}

// Elements.

/// The whole doc: built from the parsed steps each frame, with the mouse on top.
struct DocElement {
    state: Entity<DocState>,
    look: Look,
}

impl IntoElement for DocElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DocElement {
    type RequestLayoutState = Built;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Built) {
        let (parsed, selection) = {
            let state = self.state.read(cx);
            let selection = &state.selection;
            (
                state.parsed.clone(),
                (selection.start < selection.end).then_some(selection.start..selection.end),
            )
        };
        let mut scrolls = self
            .state
            .update(cx, |state, _| std::mem::take(&mut state.code_scrolls));
        let mut built = Builder::build(&self.look, &parsed, selection, &mut scrolls, window, cx);
        self.state
            .update(cx, |state, _| state.code_scrolls = scrolls);
        let child = built.element.request_layout(window, cx);
        let layout = window.request_layout(Style::default(), [child], cx);
        (layout, built)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        built: &mut Built,
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let focus = self.state.read(cx).focus.clone();
        window.set_focus_handle(&focus, cx);
        window.set_view_id(self.state.entity_id());
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        built.element.prepaint(window, cx);
        self.autoscroll(&built.rendered, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        built: &mut Built,
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut context = KeyContext::default();
        context.add("Markdown");
        window.set_key_context(context);
        let focus = self.state.read(cx).focus.clone();
        if focus.is_focused(window) {
            let input = DocInput {
                state: self.state.clone(),
                rendered: built.rendered.clone(),
            };
            window.handle_input(&focus, input, cx);
        }
        self.listen(hitbox, &built.rendered, window, cx);
        built.element.paint(window, cx);
    }
}

impl DocElement {
    fn autoscroll(&self, rendered: &Rendered, window: &mut Window, cx: &mut App) {
        let Some(source) = self.state.update(cx, |state, _| state.autoscroll.take()) else {
            return;
        };
        let Some((at, height)) = rendered.position_of(source) else {
            return;
        };
        let font = window.text_system().resolve_font(&self.look.base.font());
        let font_size = self.look.base.font_size.to_pixels(window.rem_size());
        let Ok(em) = window.text_system().em_width(font, font_size) else {
            return;
        };
        window.request_autoscroll(Bounds::from_corners(
            point(at.x - 3. * em, at.y - 3. * height),
            point(at.x + 3. * em, at.y + 3. * height),
        ));
    }

    fn listen(&self, hitbox: &Hitbox, rendered: &Rendered, window: &mut Window, cx: &mut App) {
        let pending = self.state.read(cx).selection.pending;
        let was_on_target = hitbox.is_hovered(window)
            && !pending
            && rendered
                .source_at(window.mouse_position())
                .ok()
                .is_some_and(|source| on_target(rendered, source));
        let cursor = if was_on_target {
            CursorStyle::PointingHand
        } else {
            CursorStyle::IBeam
        };
        window.set_cursor_style(cursor, hitbox);
        let state = self.state.downgrade();
        let (box_down, shown_down) = (hitbox.clone(), rendered.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            state
                .update(cx, |state, cx| {
                    state.mouse_down(event, phase, &box_down, &shown_down, window, cx);
                })
                .ok();
        });
        let state = self.state.downgrade();
        let (box_move, shown_move) = (hitbox.clone(), rendered.clone());
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            let hovered = box_move.is_hovered(window);
            state
                .update(cx, |state, cx| {
                    state.mouse_move(event, phase, hovered, &shown_move, was_on_target, cx);
                })
                .ok();
        });
        let state = self.state.downgrade();
        let shown_up = rendered.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            state
                .update(cx, |state, cx| state.mouse_up(event, phase, &shown_up, cx))
                .ok();
        });
    }
}

/// One line of text: code chips, run backgrounds, the selection, then the glyphs.
struct LineElement {
    text: StyledText,
    line: Rc<Line>,
}

impl IntoElement for LineElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LineElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.text.request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.line
            .shown
            .set(Some(bounds.intersect(&window.content_mask().bounds)));
        self.text
            .prepaint(None, inspector_id, bounds, state, window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout().clone();
        self.line.paint_chips(window);
        if layout.paint_background(window, cx).is_err() {
            return;
        }
        self.line.paint_marks(window);
        layout.paint_foreground(window, cx).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size_px(size: f32) -> AbsoluteLength {
        rems_from_px(size).into()
    }

    fn heading_sizes(look: &Look) -> Vec<Option<AbsoluteLength>> {
        look.levels.iter().map(|level| level.font_size).collect()
    }

    #[test]
    fn puts_the_doc_on_the_app_text_scale() {
        let mut look = Look::default();
        doc_text_sizes(&mut look);
        assert_eq!(look.base.font_size, size_px(13.0));
        assert_eq!(look.root_size, Some(size_px(13.0)));
        assert_eq!(look.code_block.text.font_size, Some(size_px(13.0)));
        assert_eq!(look.inline_code.font_size, Some(size_px(12.0)));
        let expected: Vec<_> = [20.0, 16.0, 15.0, 13.0, 13.0, 12.0]
            .into_iter()
            .map(|size| Some(size_px(size)))
            .collect();
        assert_eq!(heading_sizes(&look), expected);
    }

    #[test]
    fn keeps_the_heading_weight_and_gives_the_same_sizes_twice() {
        let mut look = Look::default();
        look.levels[0].font_weight = Some(FontWeight::SEMIBOLD);
        doc_text_sizes(&mut look);
        let once = heading_sizes(&look);
        doc_text_sizes(&mut look);
        assert_eq!(heading_sizes(&look), once);
        assert_eq!(look.levels[0].font_weight, Some(FontWeight::SEMIBOLD));
    }

    fn texts(pieces: &[(Range<usize>, Piece)]) -> Vec<String> {
        pieces
            .iter()
            .map(|(range, piece)| match piece {
                Piece::Start(Tag::Link { dest_url, .. }) => format!("link {range:?} {dest_url}"),
                Piece::End(TagEnd::Link) => "end link".to_owned(),
                Piece::Text { shown, plain } => format!("{range:?} {shown:?} {plain}"),
                Piece::Code(shown) => format!("code {range:?} {shown:?}"),
                Piece::Start(_) => "start".to_owned(),
                Piece::End(_) => "end".to_owned(),
                _ => "other".to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_bare_url_becomes_a_link_cut_from_the_text_around_it() {
        let parsed = Parsed::new("see https://example.com/a, then".into());
        assert_eq!(
            texts(&parsed.pieces),
            [
                "start",
                "0..4 \"see \" true",
                "link 4..25 https://example.com/a",
                "4..25 \"https://example.com/a\" true",
                "end link",
                "25..31 \", then\" true",
                "end",
            ]
        );
        assert_eq!(parsed.end(), 31);
    }

    #[test]
    fn inline_code_shows_its_source_between_the_backticks() {
        assert_eq!(code_content("`a`"), 1..2);
        assert_eq!(code_content("`` `a` ``"), 2..7);
        assert_eq!(code_content("``a`"), 0..4);
        let parsed = Parsed::new("x `a  b` y".into());
        assert_eq!(texts(&parsed.pieces)[2], "code 3..7 \"a  b\"");
    }

    #[test]
    fn smart_quotes_are_not_plain_text_and_a_footnote_starts_at_its_text() {
        let parsed = Parsed::new("a \"q\"[^n]\n\n[^n]: plain note\n".into());
        let quoted: Vec<bool> = parsed
            .pieces
            .iter()
            .filter_map(|(_, piece)| match piece {
                Piece::Text { shown, plain } if shown.contains('\u{201c}') => Some(*plain),
                _ => None,
            })
            .collect();
        assert_eq!(quoted, [false]);
        assert_eq!(parsed.note_starts.get("n").copied(), Some(17));
    }

    #[test]
    fn utf16_offsets_move_off_the_middle_of_a_character() {
        let text = "a😀b";
        assert_eq!(byte_of_utf16(text, 0, false), 0);
        assert_eq!(byte_of_utf16(text, 1, false), 1);
        assert_eq!(byte_of_utf16(text, 2, false), 1);
        assert_eq!(byte_of_utf16(text, 2, true), 5);
        assert_eq!(byte_of_utf16(text, 3, false), 5);
        assert_eq!(byte_of_utf16(text, 9, false), text.len());
    }
}
