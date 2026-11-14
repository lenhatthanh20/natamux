//! The rendered view of a stage file (`Markdown.tsx`), of a Markdown file in the editor tab and
//! of the release notes, written for Natamux (plan 6.6). A text is
//! parsed once into a tree: blocks that hold blocks or runs of styled spans. Each frame draws the
//! tree as gpui blocks and text boxes, in the look of the Markdown preview the view had before.
//! No HTML, no Mermaid and no images (an image shows its alt text); only web and mail links
//! open. A mouse selection is look only: no key copies from it (D13).

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    BorderStyle, Bounds, CursorStyle, DispatchPhase, Entity, FocusHandle, FontStyle, FontWeight,
    GlobalElementId, Hitbox, HitboxBehavior, Hsla, InputHandler, InspectorElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Point, ScrollHandle,
    StrikethroughStyle, Style, StyledText, TextAlign, TextLayout, TextRun, TextStyle,
    TextStyleRefinement, UTF16Selection, UnderlineStyle, WrappedLineLayout, point, quad, size,
};
use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use settings::Settings as _;
use theme_settings::ThemeSettings;
use ui::{Checkbox, ScrollAxes, Scrollbars, WithScrollbar, prelude::*};

use crate::app::links::open_link;

/// One view per shown file; a new text goes in with `set_text`.
pub struct MarkdownDoc {
    state: Entity<DocState>,
}

impl MarkdownDoc {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: cx.new(|cx| DocState {
                doc: Rc::default(),
                pick: Pick::default(),
                pressed: None,
                scroll_to: None,
                focus: cx.focus_handle(),
                scrolls: HashMap::new(),
            }),
        }
    }

    /// The same text does nothing, so the view does not parse or redraw.
    pub fn set_text(&self, text: &str, cx: &mut App) {
        if self.state.read(cx).doc.source.as_ref() == text {
            return;
        }
        let doc = Rc::new(Doc::parse(SharedString::from(text.to_owned())));
        self.state.update(cx, |state, cx| {
            state.doc = doc;
            cx.notify();
        });
    }

    pub fn view(&self, window: &Window, cx: &App) -> AnyElement {
        div()
            .id("docs-markdown")
            .size_full()
            .min_h_0()
            .p_4()
            .overflow_y_scroll()
            .child(self.body(window, cx))
            .into_any_element()
    }

    /// The text alone, for a parent that scrolls (the What's new dialog).
    pub fn body(&self, window: &Window, cx: &App) -> impl IntoElement + use<> {
        DocArea {
            state: self.state.clone(),
            look: Look::of(window, cx),
        }
    }
}

// The parsed doc.

/// The pulldown-cmark extensions the view reads (plan 6.6). A `+++` metadata block is parsed so
/// it can be left out.
fn options() -> Options {
    [
        Options::ENABLE_TABLES,
        Options::ENABLE_FOOTNOTES,
        Options::ENABLE_OLD_FOOTNOTES,
        Options::ENABLE_STRIKETHROUGH,
        Options::ENABLE_TASKLISTS,
        Options::ENABLE_SMART_PUNCTUATION,
        Options::ENABLE_HEADING_ATTRIBUTES,
        Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS,
        Options::ENABLE_GFM,
        Options::ENABLE_SUPERSCRIPT,
        Options::ENABLE_SUBSCRIPT,
    ]
    .into_iter()
    .fold(Options::empty(), |all, one| all | one)
}

#[derive(Default)]
struct Doc {
    source: SharedString,
    nodes: Vec<Node>,
    /// The source end of the last block. A block that ends there has no bottom margin.
    end: usize,
    /// Links (bare URLs too) and footnote references, with what a click on them opens.
    targets: Vec<Target>,
    /// Where the text of each footnote starts.
    note_text: HashMap<SharedString, usize>,
}

enum Node {
    Block(Block),
    /// Spans drawn as one text box.
    Spans(Vec<Span>),
}

struct Block {
    kind: Kind,
    range: Range<usize>,
    nodes: Vec<Node>,
}

enum Kind {
    Paragraph,
    /// h1 is 0.
    Heading(usize),
    Quote(Option<BlockQuoteKind>),
    Code,
    Html,
    /// The number of the first item, for an ordered list.
    List(Option<u64>),
    /// Whether a task item is done; none for a plain item.
    Item(Option<bool>),
    Rule,
    Note(SharedString),
    Table(Vec<Alignment>),
    Head,
    Row,
    Cell,
}

/// Text with one look: what is shown, the source it stands for, and the inline marks around it
/// from the outside in.
#[derive(Clone)]
struct Span {
    text: String,
    src: Range<usize>,
    marks: Rc<[Mark]>,
}

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Emphasis,
    Strong,
    Strike,
    Link,
    NoteRef,
    /// Inline code: always the innermost mark.
    Code,
    /// Text between `<code>` and `</code>` tags.
    CodeTag,
}

/// What a press on `range` opens: a URL, or the text of a footnote.
#[derive(Clone, PartialEq)]
struct Target {
    range: Range<usize>,
    to: SharedString,
    note: bool,
}

impl Doc {
    fn parse(source: SharedString) -> Self {
        let mut tree = Tree::default();
        if !source.is_empty() {
            tree.read(&source);
        }
        let Tree {
            top,
            end,
            targets,
            note_text,
            ..
        } = tree;
        Self {
            source,
            nodes: top,
            end,
            targets,
            note_text,
        }
    }
}

/// The tree as it grows from the events: the finished top blocks, the open blocks, the spans
/// not yet given to a block, and the open inline marks.
#[derive(Default)]
struct Tree {
    top: Vec<Node>,
    open: Vec<Block>,
    spans: Vec<Span>,
    marks: Vec<Mark>,
    end: usize,
    targets: Vec<Target>,
    note_text: HashMap<SharedString, usize>,
    /// Inside an HTML comment that spans lines.
    comment: bool,
}

/// How an inline HTML part stands to `<code>`.
enum CodeTag<'a> {
    Whole(&'a str),
    Opens,
    Closes,
    Other,
}

fn code_tag(raw: &str) -> CodeTag<'_> {
    const OPEN: &str = "<code>";
    const CLOSE: &str = "</code>";
    match raw.strip_prefix(OPEN) {
        Some(rest) => rest
            .strip_suffix(CLOSE)
            .map_or(CodeTag::Opens, CodeTag::Whole),
        None if raw.trim_end().starts_with(CLOSE) => CodeTag::Closes,
        None => CodeTag::Other,
    }
}

/// The part of an inline code span between its backticks, when it has as many at each end.
fn between_ticks(raw: &str) -> Range<usize> {
    let lead = raw.bytes().take_while(|byte| *byte == b'`').count();
    let tail = raw.bytes().rev().take_while(|byte| *byte == b'`').count();
    if lead > 0 && lead == tail && raw.len() >= lead + tail {
        lead..raw.len() - tail
    } else {
        0..raw.len()
    }
}

const LEVELS: [HeadingLevel; 6] = [
    HeadingLevel::H1,
    HeadingLevel::H2,
    HeadingLevel::H3,
    HeadingLevel::H4,
    HeadingLevel::H5,
    HeadingLevel::H6,
];

impl Tree {
    fn read(&mut self, source: &str) {
        let mut events = Parser::new_ext(source, options())
            .into_offset_iter()
            .peekable();
        let mut metadata = false;
        while let Some((event, range)) = events.next() {
            if metadata {
                metadata = !matches!(event, Event::End(TagEnd::MetadataBlock(_)));
                continue;
            }
            let raw = source.get(range.clone()).unwrap_or_default();
            match event {
                Event::Start(Tag::MetadataBlock(_)) => metadata = true,
                Event::Start(tag) => self.start(tag, range),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) if self.in_code_block() => {
                    self.text(text.into_string(), range, source, None);
                }
                Event::Text(text) => {
                    let mut run = vec![(text.into_string(), range)];
                    while let Some((Event::Text(_), _)) = events.peek() {
                        if let Some((Event::Text(more), more_range)) = events.next() {
                            run.push((more.into_string(), more_range));
                        }
                    }
                    self.words(run, source);
                }
                Event::Code(parsed) => self.code(parsed.into_string(), range, source),
                Event::Html(_) => self.html_line(raw, range),
                Event::InlineHtml(_) => match code_tag(raw) {
                    CodeTag::Whole(code) => {
                        let from = range.start + "<code>".len();
                        self.push(code.to_owned(), from..from + code.len(), Some(Mark::Code));
                    }
                    CodeTag::Opens => self.marks.push(Mark::CodeTag),
                    CodeTag::Closes => self.unmark(Mark::CodeTag),
                    CodeTag::Other => self.push(raw.to_owned(), range, None),
                },
                Event::FootnoteReference(label) => {
                    let label = SharedString::from(label.to_string());
                    self.push(format!("[{label}]"), range.clone(), Some(Mark::NoteRef));
                    self.targets.push(Target {
                        range,
                        to: label,
                        note: true,
                    });
                }
                Event::SoftBreak => self.push(" ".to_owned(), range, None),
                Event::HardBreak => self.push("\n".to_owned(), range, None),
                Event::Rule => {
                    self.begin(Kind::Rule, range);
                    self.finish();
                }
                Event::TaskListMarker(done) => {
                    let item = self
                        .open
                        .iter_mut()
                        .rev()
                        .find(|block| matches!(block.kind, Kind::Item(_)));
                    if let Some(item) = item {
                        item.kind = Kind::Item(Some(done));
                    }
                }
                Event::InlineMath(_) | Event::DisplayMath(_) => {}
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        let mark = match tag {
            Tag::Emphasis => Some(Mark::Emphasis),
            Tag::Strong => Some(Mark::Strong),
            Tag::Strikethrough => Some(Mark::Strike),
            Tag::Link { ref dest_url, .. } => {
                self.targets.push(Target {
                    range: range.clone(),
                    to: dest_url.to_string().into(),
                    note: false,
                });
                Some(Mark::Link)
            }
            _ => None,
        };
        if let Some(mark) = mark {
            self.marks.push(mark);
            return;
        }
        let kind = match tag {
            Tag::Paragraph => Kind::Paragraph,
            Tag::Heading { level, .. } => {
                Kind::Heading(LEVELS.iter().position(|at| *at == level).unwrap_or(0))
            }
            Tag::BlockQuote(kind) => Kind::Quote(kind),
            Tag::CodeBlock(_) => Kind::Code,
            Tag::HtmlBlock => Kind::Html,
            Tag::List(first) => Kind::List(first),
            Tag::Item => Kind::Item(None),
            Tag::FootnoteDefinition(label) => Kind::Note(label.to_string().into()),
            Tag::Table(aligns) => Kind::Table(aligns),
            Tag::TableHead => Kind::Head,
            Tag::TableRow => Kind::Row,
            Tag::TableCell => Kind::Cell,
            Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Link { .. }
            | Tag::Image { .. }
            | Tag::Superscript
            | Tag::Subscript
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_) => return,
        };
        self.begin(kind, range);
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Emphasis => self.unmark(Mark::Emphasis),
            TagEnd::Strong => self.unmark(Mark::Strong),
            TagEnd::Strikethrough => self.unmark(Mark::Strike),
            TagEnd::Link => self.unmark(Mark::Link),
            TagEnd::CodeBlock => {
                // The newline that ends a code block is not drawn as an empty row.
                if let Some(last) = self.spans.last_mut()
                    && last.text.ends_with('\n')
                {
                    last.text.pop();
                    last.src.end = last.src.end.saturating_sub(1);
                }
                self.finish();
            }
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell => self.finish(),
            TagEnd::Image
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn in_code_block(&self) -> bool {
        self.open
            .last()
            .is_some_and(|block| matches!(block.kind, Kind::Code))
    }

    fn unmark(&mut self, mark: Mark) {
        if let Some(at) = self.marks.iter().rposition(|open| *open == mark) {
            self.marks.remove(at);
        }
    }

    /// Gives the waiting spans to the block they belong to.
    fn settle(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.spans);
        self.add(Node::Spans(spans));
    }

    fn add(&mut self, node: Node) {
        match self.open.last_mut() {
            Some(block) => block.nodes.push(node),
            None => self.top.push(node),
        }
    }

    fn begin(&mut self, kind: Kind, range: Range<usize>) {
        self.settle();
        self.open.push(Block {
            kind,
            range,
            nodes: Vec::new(),
        });
    }

    fn finish(&mut self) {
        self.settle();
        if let Some(block) = self.open.pop() {
            self.end = block.range.end;
            self.add(Node::Block(block));
        }
    }

    fn push(&mut self, text: String, src: Range<usize>, inner: Option<Mark>) {
        let marks: Vec<Mark> = self.marks.iter().copied().chain(inner).collect();
        self.spans.push(Span {
            text,
            src,
            marks: marks.into(),
        });
    }

    /// Text from the source. The first that stands for its source byte for byte in a footnote
    /// is where a click on a reference to that footnote goes.
    fn text(&mut self, text: String, src: Range<usize>, source: &str, inner: Option<Mark>) {
        if source.get(src.clone()) == Some(text.as_str())
            && let Some(label) = self.open.iter().rev().find_map(|block| match &block.kind {
                Kind::Note(label) => Some(label.clone()),
                _ => None,
            })
        {
            self.note_text.entry(label).or_insert(src.start);
        }
        self.push(text, src, inner);
    }

    /// Text that follows text, outside a link: a bare URL in it becomes a link. The parts are
    /// joined first, so a URL is found across the cuts that smart quotes and entities make.
    fn words(&mut self, run: Vec<(String, Range<usize>)>, source: &str) {
        let joined: String = run.iter().map(|(text, _)| text.as_str()).collect();
        let urls: Vec<Range<usize>> = if self.marks.contains(&Mark::Link) {
            Vec::new()
        } else {
            let mut finder = LinkFinder::new();
            finder.kinds(&[LinkKind::Url]);
            finder
                .links(&joined)
                .map(|link| link.start()..link.end())
                .collect()
        };
        // Per URL: the source it covers, once a part of it is seen.
        let mut covers: Vec<Option<Range<usize>>> = vec![None; urls.len()];
        let mut offset = 0;
        for (text, src) in run {
            let here = offset;
            offset += text.len();
            let exact = source.get(src.clone()) == Some(text.as_str());
            // Cut only a part that stands for its source byte for byte.
            let mut cuts: Vec<usize> = urls
                .iter()
                .flat_map(|url| [url.start, url.end])
                .filter(|at| here < *at && *at < offset)
                .map(|at| at - here)
                .filter(|at| exact && text.is_char_boundary(*at))
                .collect();
            cuts.sort_unstable();
            cuts.dedup();
            cuts.push(text.len());
            let mut from = 0;
            for to in cuts {
                let piece = text.get(from..to).unwrap_or_default().to_owned();
                let piece_src = if exact {
                    src.start + from..src.start + to
                } else {
                    src.clone()
                };
                let url = urls
                    .iter()
                    .position(|url| url.start <= here + from && here + from < url.end);
                if let Some(url) = url
                    && let Some(cover) = covers.get_mut(url)
                {
                    let start = cover.as_ref().map_or(piece_src.start, |cover| cover.start);
                    *cover = Some(start..piece_src.end);
                }
                self.text(piece, piece_src, source, url.map(|_| Mark::Link));
                from = to;
            }
        }
        for (url, cover) in urls.iter().zip(covers) {
            if let (Some(cover), Some(to)) = (cover, joined.get(url.clone())) {
                self.targets.push(Target {
                    range: cover,
                    to: to.to_owned().into(),
                    note: false,
                });
            }
        }
    }

    /// Inline code shows its source between the backticks. A `\|` in a table cell is the cell's
    /// own escape, so there it shows what it stands for.
    fn code(&mut self, parsed: String, range: Range<usize>, source: &str) {
        let raw = source.get(range.clone()).unwrap_or_default();
        let inner = between_ticks(raw);
        let src = range.start + inner.start..range.start + inner.end;
        let body = source.get(src.clone()).unwrap_or_default();
        let in_table = self
            .open
            .iter()
            .any(|block| matches!(block.kind, Kind::Table(_)));
        let shown = if in_table && body.contains("\\|") {
            parsed
        } else {
            body.to_owned()
        };
        self.push(shown, src, Some(Mark::Code));
    }

    /// A line of an HTML block shows as written, except inside a comment.
    fn html_line(&mut self, raw: &str, range: Range<usize>) {
        let inside = self.comment || raw.starts_with("<!--");
        let closes = raw.trim_end().ends_with("-->");
        self.comment = inside && !closes;
        if !inside && !closes {
            self.push(raw.to_owned(), range, None);
        }
    }
}

// The look: the preview style of the old view, on the app's text scale.

#[derive(Clone, Default)]
struct Look {
    base: TextStyle,
    /// The font size of the doc's outer block.
    root_size: Option<AbsoluteLength>,
    code_text: TextStyleRefinement,
    inline_code: TextStyleRefinement,
    link: TextStyleRefinement,
    /// h1 to h6.
    heading: [TextStyleRefinement; 6],
    paragraph_gap: Pixels,
    /// The bottom margin of a list that is in no other list.
    list_gap: Pixels,
    line_height: DefiniteLength,
    cell_padding: Point<Pixels>,
    text: Hsla,
    muted: Hsla,
    border: Hsla,
    border_variant: Hsla,
    code_background: Hsla,
    chip: Hsla,
    head_background: Hsla,
    stripe_background: Hsla,
    selection: Hsla,
    /// Note, tip, important, warning and caution.
    alerts: [Hsla; 5],
}

impl Look {
    fn of(window: &Window, cx: &App) -> Self {
        let settings = ThemeSettings::get_global(cx);
        let colors = cx.theme().colors();
        let status = cx.theme().status();
        let buffer = &settings.buffer_font;
        let code_font = TextStyleRefinement {
            font_family: Some(settings.markdown_preview_code_font_family().clone()),
            font_fallbacks: buffer.fallbacks.clone(),
            font_features: Some(buffer.features.clone()),
            font_weight: Some(buffer.weight),
            ..Default::default()
        };
        let mut base = window.text_style();
        base.refine(&TextStyleRefinement {
            font_family: Some(settings.markdown_preview_font_family().clone()),
            font_fallbacks: settings.ui_font.fallbacks.clone(),
            font_features: Some(settings.ui_font.features.clone()),
            ..Default::default()
        });
        base.color = colors.text;
        base.line_height = relative(1.5);
        let weight = settings.markdown_preview_heading_font_weight();
        let heading = |color: Option<Hsla>| TextStyleRefinement {
            font_weight: Some(weight),
            line_height: Some(relative(1.25)),
            color,
            ..Default::default()
        };
        let mut look = Self {
            base,
            root_size: None,
            code_text: TextStyleRefinement {
                line_height: Some(relative(settings.buffer_line_height.value())),
                ..code_font.clone()
            },
            inline_code: TextStyleRefinement {
                color: Some(colors.text),
                ..code_font
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
            heading: [
                heading(None),
                heading(None),
                heading(None),
                heading(None),
                heading(None),
                heading(Some(colors.text_muted)),
            ],
            paragraph_gap: px(16.),
            list_gap: px(12.),
            line_height: relative(1.5),
            cell_padding: point(px(10.), px(4.)),
            text: colors.text,
            muted: colors.text_muted,
            border: colors.border,
            border_variant: colors.border_variant,
            code_background: colors.editor_background,
            chip: colors.editor_foreground.opacity(0.08),
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
        look.sizes();
        look
    }

    /// The doc on the app's text scale (`text_size.rs`, 13 px body) with the web's heading
    /// steps. Rems keep the UI zoom.
    fn sizes(&mut self) {
        let body: AbsoluteLength = rems_from_px(13.0_f32).into();
        self.base.font_size = body;
        self.root_size = Some(body);
        self.code_text.font_size = Some(body);
        self.inline_code.font_size = Some(rems_from_px(12.0_f32).into());
        let steps = [20.0_f32, 16.0, 15.0, 13.0, 13.0, 12.0];
        for (heading, step) in self.heading.iter_mut().zip(steps) {
            heading.font_size = Some(rems_from_px(step).into());
        }
    }

    /// The icon, name and colour of a GFM alert.
    fn alert(&self, kind: &BlockQuoteKind) -> (IconName, &'static str, Hsla) {
        let [note, tip, important, warning, caution] = self.alerts;
        match kind {
            BlockQuoteKind::Note => (IconName::Info, "Note", note),
            BlockQuoteKind::Tip => (IconName::Sparkle, "Tip", tip),
            BlockQuoteKind::Important => (IconName::Chat, "Important", important),
            BlockQuoteKind::Warning => (IconName::Warning, "Warning", warning),
            BlockQuoteKind::Caution => (IconName::Stop, "Caution", caution),
        }
    }

    /// The text style a mark gives, inside a link or not.
    fn mark(&self, mark: Mark, in_link: bool) -> TextStyleRefinement {
        match mark {
            Mark::Emphasis => TextStyleRefinement {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            },
            Mark::Strong => TextStyleRefinement {
                font_weight: Some(FontWeight::BOLD),
                color: Some(self.text),
                ..Default::default()
            },
            Mark::Strike => TextStyleRefinement {
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.),
                    color: None,
                }),
                ..Default::default()
            },
            Mark::Link | Mark::NoteRef => self.link.clone(),
            // The chip behind inline code is painted by its text box, not as a run background.
            Mark::Code => TextStyleRefinement {
                color: if in_link {
                    self.link.color.or(self.inline_code.color)
                } else {
                    self.inline_code.color
                },
                ..self.inline_code.clone()
            },
            Mark::CodeTag => TextStyleRefinement {
                background_color: Some(self.chip),
                ..self.inline_code.clone()
            },
        }
    }
}

fn aligned(block: Div, align: TextAlign) -> Div {
    match align {
        TextAlign::Left => block.text_left(),
        TextAlign::Center => block.text_center(),
        TextAlign::Right => block.text_right(),
    }
}

fn only_align(align: TextAlign) -> TextStyleRefinement {
    TextStyleRefinement {
        text_align: Some(align),
        ..Default::default()
    }
}

// Drawing: the tree as gpui blocks, each frame.

/// What the blocks inside a block are drawn with.
#[derive(Clone)]
struct Ctx {
    text: TextStyle,
    /// How many lists are around.
    lists: usize,
}

impl Ctx {
    fn with(&self, refinement: &TextStyleRefinement) -> Self {
        let mut text = self.text.clone();
        text.refine(refinement);
        Self {
            text,
            lists: self.lists,
        }
    }
}

struct Draw<'a> {
    look: &'a Look,
    doc: &'a Doc,
    picked: Option<Range<usize>>,
    scrolls: &'a mut HashMap<usize, ScrollHandle>,
    used_scrolls: HashSet<usize>,
    boxes: Vec<Rc<TextBox>>,
    notes_begun: bool,
}

impl Draw<'_> {
    /// The doc's first block has no top margin, and its last no bottom margin.
    fn edge<E: Styled>(&self, block: E, range: &Range<usize>) -> E {
        let block = if range.start == 0 {
            block.mt(px(0.))
        } else {
            block
        };
        if range.end == self.doc.end {
            block.mb(px(0.))
        } else {
            block
        }
    }

    fn nodes(
        &mut self,
        nodes: &[Node],
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let mut drawn = Vec::new();
        for node in nodes {
            match node {
                Node::Spans(spans) => drawn.extend(self.text_box(spans, ctx)),
                Node::Block(block) => {
                    if matches!(block.kind, Kind::Note(_)) && !self.notes_begun {
                        self.notes_begun = true;
                        let rule = div().border_t_1().mt_2().border_color(self.look.border);
                        drawn.push(self.edge(rule, &block.range).into_any_element());
                    }
                    drawn.push(self.block(block, ctx, window, cx));
                }
            }
        }
        drawn
    }

    fn block(&mut self, block: &Block, ctx: &Ctx, window: &mut Window, cx: &mut App) -> AnyElement {
        let look = self.look;
        let range = &block.range;
        match &block.kind {
            Kind::Paragraph => {
                let align = look.base.text_align;
                let inner = ctx.with(&only_align(align));
                let paragraph = div().mb(look.paragraph_gap).line_height(look.line_height);
                let paragraph = self.edge(aligned(paragraph, align), range);
                paragraph
                    .children(self.nodes(&block.nodes, &inner, window, cx))
                    .into_any_element()
            }
            Kind::Heading(level) => {
                let align = look.base.text_align;
                let own = look.heading.get(*level).cloned().unwrap_or_default();
                let runs = TextStyleRefinement {
                    color: own.color.or(Some(look.text)),
                    text_align: Some(align),
                    ..own.clone()
                };
                let mut heading = div();
                heading.style().text = TextStyleRefinement {
                    color: Some(look.text),
                    text_align: Some(align),
                    ..own
                };
                heading = match level {
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
                let heading = self.edge(heading.mt(px(24.)).mb(px(12.)), range);
                heading
                    .children(self.nodes(&block.nodes, &ctx.with(&runs), window, cx))
                    .into_any_element()
            }
            Kind::Quote(alert) => {
                let alert = alert.as_ref().map(|kind| look.alert(kind));
                let color = alert.map_or(look.border, |(_, _, color)| color);
                let mut quote = div()
                    .pl_4()
                    .mb(look.paragraph_gap)
                    .border_l_4()
                    .border_color(color);
                if let Some((icon, name, color)) = alert {
                    let color = Color::Custom(color);
                    quote = quote.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .mb_1()
                            .child(Icon::new(icon).size(IconSize::Small).color(color))
                            .child(Label::new(name).color(color).weight(FontWeight::BOLD)),
                    );
                }
                let inner = ctx.with(&TextStyleRefinement {
                    color: Some(look.muted),
                    ..Default::default()
                });
                self.edge(quote, range)
                    .children(self.nodes(&block.nodes, &inner, window, cx))
                    .into_any_element()
            }
            Kind::Code => self.code_block(block, ctx, window, cx),
            Kind::Html => self
                .edge(div(), range)
                .children(self.nodes(&block.nodes, ctx, window, cx))
                .into_any_element(),
            Kind::List(first) => {
                let list = div()
                    .pl_2p5()
                    .when(ctx.lists == 0, |list| list.mb(look.list_gap));
                let inner = Ctx {
                    lists: ctx.lists + 1,
                    ..ctx.clone()
                };
                let mut number = *first;
                let mut items = Vec::new();
                for node in &block.nodes {
                    match node {
                        Node::Block(item) if matches!(item.kind, Kind::Item(_)) => {
                            items.push(self.item(item, &mut number, &inner, window, cx));
                        }
                        other => items.extend(self.nodes(
                            std::slice::from_ref(other),
                            &inner,
                            window,
                            cx,
                        )),
                    }
                }
                self.edge(list, range).children(items).into_any_element()
            }
            Kind::Item(_) => {
                let mut number = None;
                self.item(block, &mut number, ctx, window, cx)
            }
            Kind::Rule => {
                let rule = div()
                    .border_b_1()
                    .my(look.paragraph_gap)
                    .border_color(look.border);
                self.edge(rule, range).into_any_element()
            }
            Kind::Note(label) => {
                let note = div()
                    .pt_1()
                    .mb_1()
                    .line_height(rems(1.3))
                    .text_size(rems(0.85))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .child(div().text_size(rems(0.85)).child(format!("{label}.")));
                let text = self.edge(div().flex_1().w_0(), range).children(self.nodes(
                    &block.nodes,
                    ctx,
                    window,
                    cx,
                ));
                self.edge(note, range).child(text).into_any_element()
            }
            Kind::Table(aligns) => self.table(block, aligns, ctx, window, cx),
            Kind::Head | Kind::Row | Kind::Cell => self
                .edge(div(), range)
                .children(self.nodes(&block.nodes, ctx, window, cx))
                .into_any_element(),
        }
    }

    /// The frame scrolls on its own; its scroll stays from frame to frame by where it starts.
    fn code_block(
        &mut self,
        block: &Block,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let look = self.look;
        let start = block.range.start;
        let scroll = self.scrolls.entry(start).or_default().clone();
        self.used_scrolls.insert(start);
        let bar = Scrollbars::new(ScrollAxes::Horizontal)
            .id(("doc-code-scrollbar", start))
            .tracked_scroll_handle(&scroll)
            .with_track_along(ScrollAxes::Horizontal, look.code_background)
            .notify_content();
        let mut frame = div()
            .relative()
            .w_full()
            .rounded_lg()
            .custom_scrollbars(bar, window, cx)
            .p(px(12.))
            .my(px(16.))
            .mx(px(0.))
            .border_1()
            .border_color(look.border_variant)
            .bg(look.code_background)
            .rounded(px(6.));
        frame.style().border_style = Some(BorderStyle::Solid);
        frame.style().text.refine(&look.code_text);
        let body = div()
            .id(("doc-code", start))
            .flex()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&scroll);
        let body = self.edge(body, &block.range).children(self.nodes(
            &block.nodes,
            &ctx.with(&look.code_text),
            window,
            cx,
        ));
        self.edge(frame, &block.range)
            .child(body)
            .into_any_element()
    }

    /// An item: its bullet (a number, a dot or a task box), then its blocks. A task item takes
    /// no number.
    fn item(
        &mut self,
        item: &Block,
        number: &mut Option<u64>,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let look = self.look;
        let bullet = match (&item.kind, number.as_mut()) {
            (Kind::Item(Some(done)), _) => {
                let checkbox =
                    Checkbox::new(("doc-task", item.range.start), ToggleState::from(*done))
                        .fill()
                        .visualization_only(true);
                // A task box is taller than a text bullet: it sits centred in a slot one line
                // high, so it lines up with the first line.
                let line = look
                    .line_height
                    .to_pixels(ctx.text.font_size, window.rem_size());
                div()
                    .h(line)
                    .flex()
                    .items_center()
                    .child(checkbox)
                    .into_any_element()
            }
            (_, Some(next)) => {
                let shown = format!("{next}.");
                *next += 1;
                div().child(shown).into_any_element()
            }
            (_, None) => div().child("•").into_any_element(),
        };
        let row = div()
            .mb_1()
            .gap_1()
            .line_height(look.line_height)
            .flex()
            .flex_row()
            .items_start()
            .child(bullet);
        // Without `w_0` the text does not wrap to the width of the item.
        let text = self
            .edge(div().flex_1().w_0(), &item.range)
            .children(self.nodes(&item.nodes, ctx, window, cx));
        self.edge(row, &item.range).child(text).into_any_element()
    }

    /// A grid as wide as its columns, with a scrollbar when it is wider than the view. The head
    /// is centred. Row numbers count the body rows only, so the first body row is row 0 like
    /// the head: it gets no top border and no stripe.
    fn table(
        &mut self,
        block: &Block,
        aligns: &[Alignment],
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let look = self.look;
        let start = block.range.start;
        let columns = u16::try_from(aligns.len()).unwrap_or(u16::MAX);
        let bar = Scrollbars::new(ScrollAxes::Horizontal)
            .id(("doc-table-scrollbar", start))
            .notify_content();
        let grid = div()
            .id(("doc-table", start))
            .min_w_0()
            .grid()
            .grid_cols_max_content(columns)
            .mb_2()
            .border(px(1.5))
            .border_color(look.border)
            .rounded_sm()
            .restrict_scroll_to_axis()
            .custom_scrollbars(bar, window, cx);
        let bold = ctx.with(&TextStyleRefinement {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        });
        let mut cells = Vec::new();
        let mut row = 0;
        for node in &block.nodes {
            let Node::Block(part) = node else {
                continue;
            };
            let head = matches!(part.kind, Kind::Head);
            let part_ctx = if head { &bold } else { ctx };
            let parts = part.nodes.iter().filter_map(|node| match node {
                Node::Block(cell) if matches!(cell.kind, Kind::Cell) => Some(cell),
                _ => None,
            });
            for (column, cell) in parts.enumerate() {
                let alignment = if aligns.is_empty() {
                    None
                } else if head {
                    Some(Alignment::Center)
                } else {
                    aligns.get(column).copied()
                };
                cells.push(self.cell(cell, (row, column, head), alignment, part_ctx, window, cx));
            }
            if !head {
                row += 1;
            }
        }
        let grid = self.edge(grid, &block.range).children(cells);
        self.edge(div().flex(), &block.range)
            .child(grid)
            .into_any_element()
    }

    fn cell(
        &mut self,
        cell: &Block,
        (row, column, head): (usize, usize, bool),
        alignment: Option<Alignment>,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let look = self.look;
        let align = match alignment {
            Some(Alignment::Center) => TextAlign::Center,
            Some(Alignment::Right) => TextAlign::Right,
            Some(Alignment::Left) => TextAlign::Left,
            Some(Alignment::None) | None => look.base.text_align,
        };
        let inner = ctx.with(&only_align(align));
        let mut frame = div()
            .flex()
            .flex_col()
            .h_full()
            .when(column > 0, |frame| frame.border_l_1())
            .when(row > 0, |frame| frame.border_t_1())
            .border_color(look.border)
            .px(look.cell_padding.x)
            .py(look.cell_padding.y)
            .when(head, |frame| frame.bg(look.head_background))
            .when(!head && row % 2 == 1, |frame| {
                frame.bg(look.stripe_background)
            });
        frame = match alignment {
            Some(Alignment::Center) => frame.items_center(),
            Some(Alignment::Right) => frame.items_end(),
            _ => frame,
        };
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .justify_center()
            .text_align(align);
        let body = self.edge(body, &cell.range);
        let body = match cell_task(cell) {
            // A cell that holds only `[x]`, `[X]` or `[ ]` shows a task box.
            Some(done) => {
                let checkbox = Checkbox::new(
                    ("doc-table-task", cell.range.start),
                    ToggleState::from(done),
                )
                .fill()
                .visualization_only(true);
                let line = h_flex().w_full();
                let line = match align {
                    TextAlign::Left => line.justify_start(),
                    TextAlign::Center => line.justify_center(),
                    TextAlign::Right => line.justify_end(),
                };
                body.child(line.child(checkbox))
            }
            None => body.children(self.nodes(&cell.nodes, &inner, window, cx)),
        };
        self.edge(frame, &cell.range).child(body).into_any_element()
    }

    fn text_box(&mut self, spans: &[Span], ctx: &Ctx) -> Option<AnyElement> {
        let look = self.look;
        let mut text = String::new();
        let mut runs: Vec<TextRun> = Vec::new();
        let mut segs = Vec::new();
        let mut chips = Vec::new();
        for span in spans {
            let in_link = span.marks.contains(&Mark::Link);
            let mut style = ctx.text.clone();
            for mark in span.marks.iter() {
                style.refine(&look.mark(*mark, in_link));
            }
            let at = text.len();
            text.push_str(&span.text);
            runs.push(style.to_run(span.text.len()));
            segs.push(Seg {
                at,
                len: span.text.len(),
                src: span.src.clone(),
            });
            if span.marks.last() == Some(&Mark::Code) && !span.text.is_empty() {
                chips.push(at..text.len());
            }
        }
        if text.is_empty() {
            return None;
        }
        let first = segs.first().map_or(0, |seg: &Seg| seg.src.start);
        let end = spans.last().map_or(first, |span| span.src.end);
        let picked = self.picked.as_ref().and_then(|picked| {
            let from = picked.start.max(first);
            let to = picked.end.min(end);
            (from < to).then_some(from..to)
        });
        let styled = StyledText::new(text).with_runs(runs);
        let text_box = Rc::new(TextBox {
            layout: styled.layout().clone(),
            segs,
            end,
            align: ctx.text.text_align,
            chips,
            chip: look.chip,
            picked,
            selection: look.selection,
            seen: Cell::new(None),
        });
        self.boxes.push(text_box.clone());
        Some(
            TextBoxElement {
                text: styled,
                text_box,
            }
            .into_any_element(),
        )
    }
}

/// Whether a table cell holds only a task marker, and if it is done.
fn cell_task(cell: &Block) -> Option<bool> {
    let text: String = cell
        .nodes
        .iter()
        .filter_map(|node| match node {
            Node::Spans(spans) => Some(spans),
            Node::Block(_) => None,
        })
        .flatten()
        .map(|span| span.text.as_str())
        .collect();
    match text.trim() {
        "[x]" | "[X]" => Some(true),
        "[ ]" => Some(false),
        _ => None,
    }
}

// Text boxes: where their text sits, and how a point or a source byte maps onto it.

/// Shown bytes `at..at + len` of a box stand for source `src`, byte for byte when both are as
/// long.
#[derive(Clone)]
struct Seg {
    at: usize,
    len: usize,
    src: Range<usize>,
}

struct TextBox {
    layout: TextLayout,
    segs: Vec<Seg>,
    /// The source end of its last span.
    end: usize,
    align: TextAlign,
    /// Inline code, in shown bytes.
    chips: Vec<Range<usize>>,
    chip: Hsla,
    /// The selected source in it.
    picked: Option<Range<usize>>,
    selection: Hsla,
    /// Where it was drawn, clipped; none before its first prepaint.
    seen: Cell<Option<Bounds<Pixels>>>,
}

/// One drawn row of a text box.
struct Row {
    layout: Arc<WrappedLineLayout>,
    /// Where its hard line starts in the box text.
    line_start: usize,
    /// Its bytes in the box text.
    bytes: Range<usize>,
    /// The x of its first byte in the unwrapped hard line.
    unwrapped_x: Pixels,
    top: Pixels,
    /// How far the text align moves it right.
    shift: Pixels,
}

impl TextBox {
    fn first(&self) -> usize {
        self.segs.first().map_or(self.end, |seg| seg.src.start)
    }

    fn rows(&self) -> Vec<Row> {
        let bounds = self.layout.bounds();
        let height = self.layout.line_height();
        let mut rows = Vec::new();
        let mut top = bounds.top();
        let mut line_start = 0;
        for layout in self.layout.line_layouts() {
            let flat = &layout.unwrapped_layout;
            let wraps = layout.wrap_boundaries().iter().filter_map(|wrap| {
                let glyph = flat.runs.get(wrap.run_ix)?.glyphs.get(wrap.glyph_ix)?;
                Some((glyph.index, glyph.position.x))
            });
            let mut from = (0, px(0.));
            for to in wraps.chain([(flat.len, flat.width)]) {
                let width = to.1 - from.1;
                let shift = match self.align {
                    TextAlign::Left => px(0.),
                    TextAlign::Center => (bounds.size.width - width) / 2.,
                    TextAlign::Right => bounds.size.width - width,
                };
                rows.push(Row {
                    layout: layout.clone(),
                    line_start,
                    bytes: line_start + from.0..line_start + to.0,
                    unwrapped_x: from.1,
                    top,
                    shift,
                });
                top += height;
                from = to;
            }
            line_start += layout.len() + 1;
        }
        rows
    }

    /// The box of each row piece of shown bytes `shown`.
    fn rects(&self, rows: &[Row], shown: Range<usize>) -> Vec<Bounds<Pixels>> {
        let left = self.layout.bounds().left();
        let height = self.layout.line_height();
        rows.iter()
            .filter_map(|row| {
                let from = shown.start.max(row.bytes.start);
                let to = shown.end.min(row.bytes.end);
                if from >= to {
                    return None;
                }
                let x = |at: usize| {
                    left + row.shift + row.layout.unwrapped_layout.x_for_index(at - row.line_start)
                        - row.unwrapped_x
                };
                Some(Bounds::from_corners(
                    point(x(from), row.top),
                    point(x(to), row.top + height),
                ))
            })
            .collect()
    }

    fn paint_chips(&self, rows: &[Row], window: &mut Window) {
        for chip in &self.chips {
            for row_box in self.rects(rows, chip.clone()) {
                // Wider than the glyphs by 1 px a side, and a tenth of the row in at the top and
                // the bottom, so the chip hugs them.
                let side = px(1.);
                let cut = row_box.size.height * 0.1;
                let chip_box = Bounds {
                    origin: point(row_box.origin.x - side, row_box.origin.y + cut),
                    size: size(
                        row_box.size.width + side * 2.,
                        row_box.size.height - cut * 2.,
                    ),
                };
                window.paint_quad(quad(
                    chip_box,
                    px(4.),
                    self.chip,
                    gpui::Edges::default(),
                    Hsla::transparent_black(),
                    BorderStyle::default(),
                ));
            }
        }
    }

    fn paint_picked(&self, rows: &[Row], window: &mut Window) {
        let Some(picked) = self.picked.clone() else {
            return;
        };
        let shown = self.shown_of(picked.start)..self.shown_of(picked.end);
        for row_box in self.rects(rows, shown) {
            window.paint_quad(quad(
                row_box,
                px(0.),
                self.selection,
                gpui::Edges::default(),
                Hsla::transparent_black(),
                BorderStyle::default(),
            ));
        }
    }

    fn seg_at_shown(&self, shown: usize) -> Option<&Seg> {
        self.segs.iter().rev().find(|seg| seg.at <= shown)
    }

    /// The source byte of shown byte `shown`.
    fn src_of(&self, shown: usize) -> usize {
        if shown >= self.layout.len() {
            return self.end;
        }
        match self.seg_at_shown(shown) {
            Some(seg) => (seg.src.start + (shown - seg.at)).min(seg.src.end),
            None => self.first(),
        }
    }

    /// The source end for a shown end. Where a span starts after hidden source (the backticks
    /// of inline code), it is the end of the span before.
    fn src_end_of(&self, shown: usize) -> usize {
        match self.segs.iter().position(|seg| seg.at == shown) {
            Some(at) if at > 0 && shown < self.layout.len() => self
                .segs
                .get(at - 1)
                .map_or_else(|| self.src_of(shown), |before| before.src.end),
            _ => self.src_of(shown),
        }
    }

    /// The shown byte of source byte `src`.
    fn shown_of(&self, src: usize) -> usize {
        if src >= self.end {
            return self.layout.len();
        }
        match self.segs.iter().rev().find(|seg| seg.src.start <= src) {
            Some(seg) => seg.at + (src - seg.src.start).min(seg.len),
            None => 0,
        }
    }

    /// The source byte under `position`; `Err` when it is not on a glyph.
    fn hit(&self, position: Point<Pixels>) -> Result<usize, usize> {
        let rows = self.rows();
        let row = rows
            .iter()
            .find(|row| position.y < row.top + self.layout.line_height())
            .or(rows.last());
        let shift = row.map_or(px(0.), |row| row.shift);
        match self
            .layout
            .index_for_position(point(position.x - shift, position.y))
        {
            Ok(shown) => Ok(self.src_of(shown)),
            Err(shown) => Err(self.src_of(shown)),
        }
    }
}

/// The text boxes of one frame and the doc they show.
#[derive(Clone)]
struct Hits {
    boxes: Rc<[Rc<TextBox>]>,
    doc: Rc<Doc>,
}

/// Space, punctuation or word, for a double click. Word wins over the others, punctuation over
/// space.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Space,
    Punctuation,
    Word,
}

fn class(c: char) -> Class {
    if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else if c.is_whitespace() {
        Class::Space
    } else {
        Class::Punctuation
    }
}

impl Hits {
    fn drawn(&self) -> impl Iterator<Item = &Rc<TextBox>> {
        self.boxes
            .iter()
            .filter(|text_box| text_box.seen.get().is_some())
    }

    fn end(&self) -> usize {
        self.boxes.last().map_or(0, |text_box| text_box.end)
    }

    /// The drawn box that holds source byte `src`.
    fn box_of(&self, src: usize) -> Option<&Rc<TextBox>> {
        self.drawn().find(|text_box| src <= text_box.end)
    }

    /// The source byte under `position`. A point in no box goes to the box below it, or to the
    /// end of the box above when it is between two.
    fn hit(&self, position: Point<Pixels>) -> Result<usize, usize> {
        let drawn: Vec<&Rc<TextBox>> = self.drawn().collect();
        // Table columns share rows, so a box that holds the point wins first.
        if let Some(text_box) = drawn
            .iter()
            .find(|text_box| text_box.layout.bounds().contains(&position))
        {
            return text_box.hit(position);
        }
        let below = drawn
            .iter()
            .position(|text_box| position.y <= text_box.layout.bounds().bottom());
        match below {
            Some(at) => {
                let text_box = drawn.get(at).copied();
                let above = at.checked_sub(1).and_then(|before| drawn.get(before));
                match (text_box, above) {
                    (Some(text_box), Some(above))
                        if position.y < text_box.layout.bounds().top() =>
                    {
                        Err(above.end)
                    }
                    (Some(text_box), _) => text_box.hit(position),
                    (None, _) => Err(self.end()),
                }
            }
            None => Err(self.end()),
        }
    }

    fn target_at(&self, src: usize) -> Option<Target> {
        let targets = &self.doc.targets;
        let notes = targets.iter().filter(|target| target.note);
        let links = targets.iter().filter(|target| !target.note);
        notes
            .chain(links)
            .find(|target| target.range.contains(&src))
            .cloned()
    }

    fn word_at(&self, src: usize) -> Range<usize> {
        let Some(text_box) = self.box_of(src) else {
            return src..src;
        };
        let text = text_box.layout.text();
        let at = text_box.shown_of(src).min(text.len());
        let (Some(head), Some(tail)) = (text.get(..at), text.get(at..)) else {
            return src..src;
        };
        let kind = head
            .chars()
            .next_back()
            .map(class)
            .max(tail.chars().next().map(class));
        let same = |c: char| Some(class(c)) == kind;
        let start = head
            .char_indices()
            .rev()
            .take_while(|(_, c)| same(*c))
            .last()
            .map_or(at, |(from, _)| from);
        let end = tail
            .char_indices()
            .find(|(_, c)| !same(*c))
            .map_or(text.len(), |(to, _)| at + to);
        text_box.src_of(start)..text_box.src_end_of(end)
    }

    fn box_range_at(&self, src: usize) -> Range<usize> {
        self.box_of(src)
            .map_or(src..src, |text_box| text_box.first()..text_box.end)
    }

    fn text_of(&self, range: Range<usize>) -> String {
        self.drawn()
            .filter(|text_box| range.start <= text_box.end && text_box.first() <= range.end)
            .map(|text_box| {
                let text = text_box.layout.text();
                let from = text_box.shown_of(range.start.max(text_box.first()));
                let to = text_box.shown_of(range.end.min(text_box.end));
                text.get(from..to.max(from)).unwrap_or_default().to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn rects_of(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.drawn()
            .filter(|text_box| range.start < text_box.end && text_box.first() < range.end)
            .flat_map(|text_box| {
                let from = text_box.shown_of(range.start.max(text_box.first()));
                let to = text_box.shown_of(range.end.min(text_box.end));
                text_box.rects(&text_box.rows(), from..to)
            })
            .collect()
    }

    fn point_of(&self, src: usize) -> Option<(Point<Pixels>, Pixels)> {
        let text_box = self.box_of(src)?;
        let shown = text_box.shown_of(src.max(text_box.first()));
        let at = text_box.layout.position_for_index(shown)?;
        Some((at, text_box.layout.line_height()))
    }

    /// The drawn text for the platform's text queries: the boxes one per line, each with where
    /// it starts in that text.
    fn flat(&self) -> (String, Vec<(usize, &Rc<TextBox>)>) {
        let mut text = String::new();
        let mut starts = Vec::new();
        for text_box in self.drawn() {
            if !starts.is_empty() {
                text.push('\n');
            }
            starts.push((text.len(), text_box));
            text.push_str(&text_box.layout.text());
        }
        (text, starts)
    }

    fn utf16_of(&self, src: usize) -> usize {
        let (text, starts) = self.flat();
        let byte = starts
            .iter()
            .find(|(_, text_box)| src <= text_box.end)
            .map_or(text.len(), |(start, text_box)| {
                start + text_box.shown_of(src)
            });
        utf16_len(text.get(..byte).unwrap_or(&text))
    }

    fn src_of_utf16(&self, units: usize) -> usize {
        let (text, starts) = self.flat();
        let byte = byte_of_utf16(&text, units, false);
        let found = starts.iter().rev().find(|(start, _)| *start <= byte);
        match found {
            Some((start, text_box)) => text_box.src_of(byte - start),
            None => self.end(),
        }
    }
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The byte where `units` UTF-16 units of `text` end. A count that ends inside a character
/// goes back to its start, or on to its end with `round_up`.
fn byte_of_utf16(text: &str, units: usize, round_up: bool) -> usize {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen == units {
            return byte;
        }
        seen += c.len_utf16();
        if seen > units {
            return if round_up { byte + c.len_utf8() } else { byte };
        }
    }
    text.len()
}

// The selection.

#[derive(Clone, Copy, Default, PartialEq)]
enum Unit {
    #[default]
    Char,
    Word,
    Block,
    All,
}

/// A mouse selection in source bytes: what the press took, grown to what the pointer reaches
/// in the same unit.
#[derive(Clone, Default)]
struct Pick {
    anchor: Range<usize>,
    unit: Unit,
    range: Range<usize>,
    /// The pointer is before the anchor.
    reversed: bool,
    /// The button is still down.
    held: bool,
}

impl Pick {
    fn new(anchor: Range<usize>, unit: Unit) -> Self {
        Self {
            range: anchor.clone(),
            anchor,
            unit,
            reversed: false,
            held: true,
        }
    }

    /// Where it stays when shift extends it.
    fn fixed_end(&self) -> usize {
        if self.reversed {
            self.range.end
        } else {
            self.range.start
        }
    }

    /// Grows it from the anchor to `reach`, the unit under the pointer.
    fn reach(&mut self, reach: Range<usize>) {
        self.reversed = reach.start < self.anchor.start;
        self.range = self.anchor.start.min(reach.start)..self.anchor.end.max(reach.end);
    }

    fn selected(&self) -> Option<Range<usize>> {
        (self.range.start < self.range.end).then(|| self.range.clone())
    }
}

fn unit_at(unit: Unit, src: usize, hits: &Hits) -> Range<usize> {
    match unit {
        Unit::Char => src..src,
        Unit::Word => hits.word_at(src),
        Unit::Block => hits.box_range_at(src),
        Unit::All => 0..hits.end(),
    }
}

struct DocState {
    doc: Rc<Doc>,
    pick: Pick,
    /// The link or footnote reference under the last press.
    pressed: Option<Target>,
    /// A source byte to scroll into view at the next frame.
    scroll_to: Option<usize>,
    focus: FocusHandle,
    /// The side scroll of each code block, by the block's source start.
    scrolls: HashMap<usize, ScrollHandle>,
}

impl DocState {
    fn press(
        &mut self,
        event: &MouseDownEvent,
        phase: DispatchPhase,
        hitbox: &Hitbox,
        hits: &Hits,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !hitbox.is_hovered(window) {
            // A press anywhere else drops the selection.
            if phase.capture() && event.button == MouseButton::Left {
                self.pick = Pick::default();
                self.pressed = None;
                cx.notify();
            }
            return;
        }
        if !phase.bubble() || event.button == MouseButton::Right {
            return;
        }
        let hit = hits.hit(event.position);
        self.pressed = hit.ok().and_then(|src| hits.target_at(src));
        if self.pressed.is_none() {
            let (Ok(src) | Err(src)) = hit;
            self.pick = match event.click_count {
                1 if event.modifiers.shift => {
                    let fixed = self.pick.fixed_end();
                    let mut pick = Pick::new(fixed..fixed, Unit::Char);
                    pick.reach(src..src);
                    pick
                }
                1 => Pick::new(src..src, Unit::Char),
                2 => Pick::new(hits.word_at(src), Unit::Word),
                3 => Pick::new(hits.box_range_at(src), Unit::Block),
                _ => Pick::new(0..hits.end(), Unit::All),
            };
            window.focus(&self.focus, cx);
        }
        window.prevent_default();
        cx.notify();
    }

    fn drag(
        &mut self,
        event: &MouseMoveEvent,
        phase: DispatchPhase,
        hovered: bool,
        hits: &Hits,
        was_over_target: bool,
        cx: &mut Context<Self>,
    ) {
        if !phase.bubble() {
            return;
        }
        if self.pick.held {
            let (Ok(src) | Err(src)) = hits.hit(event.position);
            self.pick.reach(unit_at(self.pick.unit, src, hits));
            self.scroll_code_sideways(event.position);
            self.scroll_to = Some(src);
            cx.notify();
            return;
        }
        let over_target = hovered
            && hits
                .hit(event.position)
                .ok()
                .and_then(|src| hits.target_at(src))
                .is_some();
        // Redraw for the pointer shape.
        if over_target != was_over_target {
            cx.notify();
        }
    }

    fn release(
        &mut self,
        event: &MouseUpEvent,
        phase: DispatchPhase,
        hits: &Hits,
        cx: &mut Context<Self>,
    ) {
        if phase.capture() {
            if self.pick.held {
                self.pick.held = false;
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                cx.write_to_primary(gpui::ClipboardItem::new_string(
                    hits.text_of(self.pick.range.clone()),
                ));
                cx.notify();
            }
            return;
        }
        let Some(pressed) = self.pressed.take() else {
            return;
        };
        let released = hits
            .hit(event.position)
            .ok()
            .and_then(|src| hits.target_at(src));
        if released.as_ref() != Some(&pressed) {
            return;
        }
        if pressed.note {
            if let Some(start) = self.doc.note_text.get(&pressed.to) {
                self.scroll_to = Some(*start);
                cx.notify();
            }
        } else {
            open_link(&pressed.to, cx);
        }
    }

    /// While selecting, a pointer left or right of a code block, at its height, scrolls it
    /// that way.
    fn scroll_code_sideways(&self, position: Point<Pixels>) {
        let beside = self.scrolls.values().find(|scroll| {
            let bounds = scroll.bounds();
            (bounds.top()..=bounds.bottom()).contains(&position.y)
        });
        let Some(scroll) = beside else {
            return;
        };
        let bounds = scroll.bounds();
        // How far the pointer is outside the block, as a move toward it.
        let by = position.x.max(bounds.left()).min(bounds.right()) - position.x;
        if by != px(0.) {
            let offset = scroll.offset();
            scroll.set_offset(point(offset.x + by, offset.y));
        }
    }
}

/// The doc for the platform's text queries (look up, services). It takes no typing.
struct DocInput {
    state: Entity<DocState>,
    hits: Hits,
}

impl InputHandler for DocInput {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        let pick = &self.state.read(cx).pick;
        Some(UTF16Selection {
            range: self.hits.utf16_of(pick.range.start)..self.hits.utf16_of(pick.range.end),
            reversed: pick.reversed,
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
        let (text, _) = self.hits.flat();
        let from = byte_of_utf16(&text, range_utf16.start, false);
        let to = byte_of_utf16(&text, range_utf16.end, true);
        let part = text.get(from..to).unwrap_or_default();
        let start = utf16_len(text.get(..from).unwrap_or_default());
        let given = start..start + utf16_len(part);
        if given != range_utf16 {
            *adjusted_range = Some(given);
        }
        Some(part.to_owned())
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
        let from = self.hits.src_of_utf16(range_utf16.start);
        let to = self.hits.src_of_utf16(range_utf16.end);
        if let Some(first) = self.hits.rects_of(from..to).into_iter().next() {
            return Some(first);
        }
        let (origin, height) = self.hits.point_of(from)?;
        Some(Bounds {
            origin,
            size: size(px(0.), height),
        })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        let text_box = self.hits.drawn().find(|text_box| {
            text_box
                .seen
                .get()
                .is_some_and(|seen| seen.contains(&point))
        })?;
        let src = text_box.hit(point).ok()?;
        Some(self.hits.utf16_of(src))
    }

    fn accepts_text_input(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        false
    }
}

// Elements.

/// The whole doc, drawn from the tree each frame, with the mouse on top.
struct DocArea {
    state: Entity<DocState>,
    look: Look,
}

struct Frame {
    root: AnyElement,
    hits: Hits,
}

impl IntoElement for DocArea {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DocArea {
    type RequestLayoutState = Frame;
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
    ) -> (LayoutId, Frame) {
        let (doc, picked) = {
            let state = self.state.read(cx);
            (state.doc.clone(), state.pick.selected())
        };
        let mut scrolls = self
            .state
            .update(cx, |state, _| std::mem::take(&mut state.scrolls));
        let mut draw = Draw {
            look: &self.look,
            doc: &doc,
            picked,
            scrolls: &mut scrolls,
            used_scrolls: HashSet::new(),
            boxes: Vec::new(),
            notes_begun: false,
        };
        let ctx = Ctx {
            text: self.look.base.clone(),
            lists: 0,
        };
        let blocks = draw.nodes(&doc.nodes, &ctx, window, cx);
        let Draw {
            used_scrolls,
            boxes,
            ..
        } = draw;
        scrolls.retain(|start, _| used_scrolls.contains(start));
        self.state.update(cx, |state, _| state.scrolls = scrolls);
        let mut root = div();
        root.style().text.font_size = self.look.root_size;
        let mut root = root.children(blocks).into_any_element();
        let child = root.request_layout(window, cx);
        let layout = window.request_layout(Style::default(), [child], cx);
        let hits = Hits {
            boxes: boxes.into(),
            doc,
        };
        (layout, Frame { root, hits })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        // Changes of the state redraw the view that holds this element.
        window.set_view_id(self.state.entity_id());
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let focus = self.state.read(cx).focus.clone();
        window.set_focus_handle(&focus, cx);
        frame.root.prepaint(window, cx);
        self.scroll_into_view(&frame.hits, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        frame: &mut Frame,
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.state.read(cx).focus.clone();
        if focus.is_focused(window) {
            let input = DocInput {
                state: self.state.clone(),
                hits: frame.hits.clone(),
            };
            window.handle_input(&focus, input, cx);
        }
        self.listen(hitbox, &frame.hits, window, cx);
        frame.root.paint(window, cx);
    }
}

impl DocArea {
    /// Brings the asked source byte into view with three lines and three em of room around it.
    fn scroll_into_view(&self, hits: &Hits, window: &mut Window, cx: &mut App) {
        let Some(src) = self.state.update(cx, |state, _| state.scroll_to.take()) else {
            return;
        };
        let Some((at, line)) = hits.point_of(src) else {
            return;
        };
        let font = window.text_system().resolve_font(&self.look.base.font());
        let font_size = self.look.base.font_size.to_pixels(window.rem_size());
        let Ok(em) = window.text_system().em_width(font, font_size) else {
            return;
        };
        let (wide, high) = (em * 3., line * 3.);
        window.request_autoscroll(Bounds::from_corners(
            point(at.x - wide, at.y - high),
            point(at.x + wide, at.y + high),
        ));
    }

    fn listen(&self, hitbox: &Hitbox, hits: &Hits, window: &mut Window, cx: &mut App) {
        let held = self.state.read(cx).pick.held;
        let over_target = hitbox.is_hovered(window)
            && !held
            && hits
                .hit(window.mouse_position())
                .ok()
                .and_then(|src| hits.target_at(src))
                .is_some();
        let cursor = if over_target {
            CursorStyle::PointingHand
        } else {
            CursorStyle::IBeam
        };
        window.set_cursor_style(cursor, hitbox);
        let (state, area, frame) = (self.state.downgrade(), hitbox.clone(), hits.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            state
                .update(cx, |state, cx| {
                    state.press(event, phase, &area, &frame, window, cx);
                })
                .ok();
        });
        let (state, area, frame) = (self.state.downgrade(), hitbox.clone(), hits.clone());
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            let hovered = area.is_hovered(window);
            state
                .update(cx, |state, cx| {
                    state.drag(event, phase, hovered, &frame, over_target, cx);
                })
                .ok();
        });
        let (state, frame) = (self.state.downgrade(), hits.clone());
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            state
                .update(cx, |state, cx| state.release(event, phase, &frame, cx))
                .ok();
        });
    }
}

/// One text box: code chips, run backgrounds, the selection, then the glyphs.
struct TextBoxElement {
    text: StyledText,
    text_box: Rc<TextBox>,
}

impl IntoElement for TextBoxElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextBoxElement {
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
        let seen = bounds.intersect(&window.content_mask().bounds);
        self.text_box.seen.set(Some(seen));
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
        let text_box = &self.text_box;
        let rows = if text_box.chips.is_empty() && text_box.picked.is_none() {
            Vec::new()
        } else {
            text_box.rows()
        };
        text_box.paint_chips(&rows, window);
        if text_box.layout.paint_background(window, cx).is_err() {
            return;
        }
        text_box.paint_picked(&rows, window);
        text_box.layout.paint_foreground(window, cx).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rems_of(size: f32) -> AbsoluteLength {
        rems_from_px(size).into()
    }

    fn heading_sizes(look: &Look) -> Vec<Option<AbsoluteLength>> {
        look.heading.iter().map(|level| level.font_size).collect()
    }

    #[test]
    fn puts_the_doc_on_the_app_text_scale() {
        let mut look = Look::default();
        look.sizes();
        assert_eq!(look.base.font_size, rems_of(13.0));
        assert_eq!(look.root_size, Some(rems_of(13.0)));
        assert_eq!(look.code_text.font_size, Some(rems_of(13.0)));
        assert_eq!(look.inline_code.font_size, Some(rems_of(12.0)));
        let expected: Vec<_> = [20.0, 16.0, 15.0, 13.0, 13.0, 12.0]
            .into_iter()
            .map(|size| Some(rems_of(size)))
            .collect();
        assert_eq!(heading_sizes(&look), expected);
    }

    #[test]
    fn keeps_the_heading_weight_and_gives_the_same_sizes_twice() {
        let mut look = Look::default();
        look.heading[0].font_weight = Some(FontWeight::SEMIBOLD);
        look.sizes();
        let once = heading_sizes(&look);
        look.sizes();
        assert_eq!(heading_sizes(&look), once);
        assert_eq!(look.heading[0].font_weight, Some(FontWeight::SEMIBOLD));
    }

    /// Every span of a doc, in order: its source range, text, and whether it is in a link or
    /// inline code.
    fn spans(doc: &Doc) -> Vec<String> {
        fn walk(nodes: &[Node], out: &mut Vec<String>) {
            for node in nodes {
                match node {
                    Node::Block(block) => walk(&block.nodes, out),
                    Node::Spans(spans) => out.extend(spans.iter().map(|span| {
                        let link = if span.marks.contains(&Mark::Link) {
                            " link"
                        } else {
                            ""
                        };
                        let code = if span.marks.contains(&Mark::Code) {
                            " code"
                        } else {
                            ""
                        };
                        format!("{:?} {:?}{link}{code}", span.src, span.text)
                    })),
                }
            }
        }
        let mut out = Vec::new();
        walk(&doc.nodes, &mut out);
        out
    }

    #[test]
    fn a_bare_url_becomes_a_link_cut_from_the_text_around_it() {
        let doc = Doc::parse("see https://example.com/a, then".into());
        assert_eq!(
            spans(&doc),
            [
                "0..4 \"see \"",
                "4..25 \"https://example.com/a\" link",
                "25..31 \", then\"",
            ]
        );
        let links: Vec<_> = doc
            .targets
            .iter()
            .map(|target| (target.range.clone(), target.to.to_string(), target.note))
            .collect();
        assert_eq!(links, [(4..25, "https://example.com/a".to_owned(), false)]);
        assert_eq!(doc.end, 31);
    }

    #[test]
    fn inline_code_shows_its_source_between_the_backticks() {
        assert_eq!(between_ticks("`a`"), 1..2);
        assert_eq!(between_ticks("`` `a` ``"), 2..7);
        assert_eq!(between_ticks("``a`"), 0..4);
        let doc = Doc::parse("x `a  b` y".into());
        assert_eq!(
            spans(&doc).get(1).map(String::as_str),
            Some("3..7 \"a  b\" code")
        );
    }

    #[test]
    fn smart_quotes_are_not_their_source_and_a_footnote_starts_at_its_text() {
        let source = "a \"q\"[^n]\n\n[^n]: plain note\n";
        let doc = Doc::parse(source.into());
        let mut quoted = Vec::new();
        fn walk(nodes: &[Node], source: &str, out: &mut Vec<bool>) {
            for node in nodes {
                match node {
                    Node::Block(block) => walk(&block.nodes, source, out),
                    Node::Spans(spans) => out.extend(
                        spans
                            .iter()
                            .filter(|span| span.text.contains('\u{201c}'))
                            .map(|span| source.get(span.src.clone()) == Some(span.text.as_str())),
                    ),
                }
            }
        }
        walk(&doc.nodes, source, &mut quoted);
        assert_eq!(quoted, [false]);
        assert_eq!(doc.note_text.get("n").copied(), Some(17));
        assert!(
            doc.targets
                .iter()
                .any(|target| target.note && target.to.as_ref() == "n")
        );
    }

    #[test]
    fn an_html_comment_is_hidden_and_other_html_shows_as_written() {
        let doc = Doc::parse("<!--\nhidden\n-->\n\n<b>shown</b>\n".into());
        let texts = spans(&doc).join(" ");
        assert!(!texts.contains("hidden"), "{texts}");
        assert!(texts.contains("<b>") && texts.contains("shown"), "{texts}");
        assert!(matches!(code_tag("<code>a</code>"), CodeTag::Whole("a")));
        assert!(matches!(code_tag("<code>"), CodeTag::Opens));
        assert!(matches!(code_tag("</code> "), CodeTag::Closes));
        assert!(matches!(code_tag("<b>"), CodeTag::Other));
    }

    #[test]
    fn a_task_item_has_its_state_and_takes_no_number() {
        let doc = Doc::parse("1. [x] done\n2. next\n".into());
        let items = match doc.nodes.first() {
            Some(Node::Block(list)) => list.nodes.as_slice(),
            _ => &[],
        };
        let states: Vec<Option<bool>> = items
            .iter()
            .filter_map(|node| match node {
                Node::Block(Block {
                    kind: Kind::Item(done),
                    ..
                }) => Some(*done),
                _ => None,
            })
            .collect();
        assert_eq!(states, [Some(true), None]);
    }

    #[test]
    fn a_pick_grows_from_its_anchor_in_both_directions() {
        let mut pick = Pick::new(4..8, Unit::Word);
        pick.reach(0..3);
        assert_eq!((pick.range.clone(), pick.reversed), (0..8, true));
        assert_eq!(pick.fixed_end(), 8);
        pick.reach(5..7);
        assert_eq!((pick.range.clone(), pick.reversed), (4..8, false));
        pick.reach(10..12);
        assert_eq!((pick.range.clone(), pick.reversed), (4..12, false));
        assert_eq!(pick.selected(), Some(4..12));
        assert_eq!(Pick::new(3..3, Unit::Char).selected(), None);
    }

    #[test]
    fn utf16_counts_move_off_the_middle_of_a_character() {
        let text = "a😀b";
        assert_eq!(byte_of_utf16(text, 0, false), 0);
        assert_eq!(byte_of_utf16(text, 1, false), 1);
        assert_eq!(byte_of_utf16(text, 2, false), 1);
        assert_eq!(byte_of_utf16(text, 2, true), 5);
        assert_eq!(byte_of_utf16(text, 3, false), 5);
        assert_eq!(byte_of_utf16(text, 9, false), text.len());
        assert_eq!(utf16_len(text), 4);
    }
}
