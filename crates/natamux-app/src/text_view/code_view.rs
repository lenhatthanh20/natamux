//! The code view (plan 6.4, S4.3b): the own text input in full mode, plus what code needs:
//! - syntax colours from the own highlighter, parsed on the main thread within a budget and
//!   finished on a background thread when that runs out;
//! - indent guides and the active guide;
//! - the whitespace marks of the whole text while the diff's Show whitespace is on (that switch
//!   was Zed's `show_whitespaces` for every editor);
//! - the matching bracket and the other places of the selected text;
//! - the editor's scrollbar with its markers, one page of scroll past the end and a scroll margin
//!   of 3 lines (keys and edits only: a click does not scroll);
//! - the typing facts of its language (`code_rules.rs`, `code_typing.rs`).
//!
//! The scrollbar is in `code_bar.rs`, what it paints in `code_paint.rs`, the colours in
//! `code_runs.rs`, the gutter and the sticky rows in `code_gutter.rs`, and what the editor tab
//! has more (no wrap, blame labels, the right-click menu) in `code_editor.rs`.
//!
//! The JSON tab and the Docs views use it; the editor tab follows in S4.3c.

use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Pixels, Task, Window, px};
use tree_sitter::Tree;

use super::code_bar::{BAR_WIDTH, Bar};
use super::code_editor::{Labels, OpenMenu};
use super::code_gutter::Caches;
use super::code_langs::typing_for;
use super::code_rules::{Typing, find_all};
use super::code_runs::{RunCache, input_edit};
use super::code_typing::Region;
use super::element::Layout;
use super::highlight::{PARSE_BUDGET, Syntax};
use super::text_box::TextBox;
use super::text_input::{Mode, TextInput};
use crate::app::appearance::Appearance;
use crate::git::code_target::RowMenu;
use crate::git::languages::{Lang, lang_for};
use crate::text::model::TextModel;

/// The scroll margin of the editor, in lines.
const SCROLL_MARGIN: f32 = 3.;
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

    pub(super) fn input_ref(&self) -> &Entity<TextInput> {
        &self.input
    }

    pub(super) fn input_id(&self) -> gpui::EntityId {
        self.input.entity_id()
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
        let base = gpui::TextRun {
            len: 0,
            font: gpui::font(".ZedMono"),
            color: gpui::Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = input
            .code_runs(&base, input.model.text().len(), None, cx)
            .unwrap_or_default();
        let mut colors: Vec<gpui::Hsla> = Vec::new();
        for run in runs {
            if !colors.contains(&run.color) {
                colors.push(run.color);
            }
        }
        colors.len()
    }
}

/// What a text input has more as a code view.
pub struct Code {
    pub(super) lang: Option<&'static Lang>,
    pub(super) guides: bool,
    /// Line numbers left of the text (the editor tab).
    pub(super) gutter: bool,
    /// The rows of the outline items the top row is inside stay pinned at the top.
    pub(super) sticky: bool,
    /// Long lines wrap; off, the view scrolls sideways (the editor tab's toggle).
    pub(super) wrap: bool,
    pub(super) syntax: Option<Syntax>,
    /// The text the tree was last moved to.
    parsed: Arc<str>,
    /// Goes up when the colours change with the text the same (a parse landed).
    pub(super) colors: u64,
    _parse: Option<Task<()>>,
    pub(super) regions: Vec<Region>,
    /// The text version the regions belong to: any other edit drops them.
    pub(super) regions_version: u64,
    pub(super) bar: Bar,
    /// The places of the selected text (the selection among them), the selected text and the
    /// text version they are of.
    pub(super) occurrences: Vec<Range<usize>>,
    occurrences_of: Option<String>,
    occurrences_version: u64,
    _occurrences: Option<Task<()>>,
    /// The matching brackets, found at the last change of the selection, and the text version
    /// they are of.
    pub(super) brackets: Vec<Range<usize>>,
    pub(super) brackets_version: u64,
    /// The matches find shows, and the active one. Offsets of the text they were found in: find
    /// searches again after each edit and shows them anew.
    pub(super) search: Vec<Range<usize>>,
    pub(super) search_active: Option<usize>,
    /// Model version the matches were found at; older matches are not painted.
    pub(super) search_version: u64,
    pub(super) run_cache: RefCell<RunCache>,
    /// The guides and the outline, kept until the text changes.
    pub(super) caches: RefCell<Caches>,
    /// Blame labels at the start of their rows.
    pub(super) labels: Labels,
    /// Builds the right-click menu for a row; the open menu.
    pub(super) menu: Option<RowMenu>,
    pub(super) open_menu: Option<OpenMenu>,
}

impl Code {
    fn new(lang: Option<&'static Lang>, guides: bool) -> Self {
        Code {
            lang,
            guides,
            gutter: false,
            sticky: false,
            wrap: true,
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
            search: Vec::new(),
            search_active: None,
            search_version: 0,
            run_cache: RefCell::default(),
            caches: RefCell::default(),
            labels: Labels::default(),
            menu: None,
            open_menu: None,
        }
    }

    pub(super) fn tree(&self) -> Option<&Tree> {
        self.syntax.as_ref()?.tree()
    }

    pub(super) fn typing(&self) -> Option<&'static Typing> {
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
        code.labels.place(text);
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
}
