//! One side of the own diff view (plan 6.5, S6.2 part 2): a read-only code view whose text is the
//! rows the side shows (`diff_model::side_rows` or `inline_rows`), with:
//! - the line numbers of the side's file, none on deleted lines, added lines in the added colour;
//! - empty rows: the spacers that keep the two sides level (as tall as their rows, T-0230), and
//!   one row with a line in its middle for the unchanged lines between two collapsed parts;
//! - the rows of each hunk in a box: the hollow background of their kind with a line at its top
//!   and its bottom, a strip in the gutter in the colour of the hunk's kind, and a marker where a
//!   hunk with no rows on this side lies;
//! - the changed words of small hunks;
//! - the syntax colours of each line from the parse of its own file, drawn at once and again when
//!   a parse ends, so deleted lines always end coloured (advisor A5);
//! - one scroll for the two sides of a side by side diff.
//!
//! The look values were measured on the `changes-diff-*` shots of B2. The diff view moves onto it
//! in S6.2 part 3; until then only its tests use it.

#![cfg_attr(not(test), allow(dead_code))]

use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, AppContext as _, Context, Entity, Pixels, SharedString, Task, TextRun, WeakEntity, Window,
};
use theme::ActiveTheme;

use super::code_runs::styled_runs;
use super::code_view::Code;
use super::highlight::{Highlights, PARSE_BUDGET, Syntax};
use super::text_box::TextBox;
use super::text_input::TextInput;
use crate::git::diff_model::{FileDiff, Row};
use crate::git::diff_rows::{SideRows, line_ranges, side_rows};
use crate::git::languages::lang_for;
use crate::text::model::TextModel;

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
    /// The last commit's file and the working tree's.
    sources: [Option<Source>; 2],
    cache: RefCell<RunCache>,
    /// The other side of a side by side diff, which scrolls with this one.
    peer: Option<WeakEntity<TextInput>>,
    /// The scroll both sides were last set to.
    synced: Option<Pixels>,
    /// The scroll this side was last laid out at, for the tests.
    #[cfg(test)]
    pub(super) drawn: Option<Pixels>,
    _parses: Vec<Task<()>>,
}

impl DiffDecor {
    fn new() -> Self {
        DiffDecor {
            rows: SideRows::empty(),
            sources: [None, None],
            cache: RefCell::default(),
            peer: None,
            synced: None,
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

    /// After a scroll of a diff side: the other side takes the same scroll, as both have the same
    /// rows.
    pub(super) fn sync_peer(&mut self, cx: &mut Context<Self>) {
        let y = self.scroll.y;
        let Some(diff) = self.code.as_mut().and_then(|code| code.diff.as_mut()) else {
            return;
        };
        #[cfg(test)]
        {
            diff.drawn = Some(y);
        }
        if diff.synced == Some(y) {
            return;
        }
        diff.synced = Some(y);
        let Some(peer) = diff.peer.as_ref().and_then(WeakEntity::upgrade) else {
            return;
        };
        let moved = peer.update(cx, |peer, _| {
            if let Some(diff) = peer.code.as_mut().and_then(|code| code.diff.as_mut()) {
                diff.synced = Some(y);
            }
            let moved = peer.scroll.y != y;
            peer.scroll.y = y;
            moved
        });
        // This runs in a draw, where gpui keeps a notify from drawing the window again; the peer
        // may already be laid out at its old scroll, so ask for its new draw after this frame.
        if moved {
            cx.defer(move |cx| peer.update(cx, |_, cx| cx.notify()));
        }
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

    /// Shows `rows` of `diff` (see `side_rows`), coloured in the language of `path`. A newer read
    /// of the same file changes only the lines that differ, so the caret stays.
    pub fn show(&self, path: &str, diff: &FileDiff, rows: &[Row], new_side: bool, cx: &mut App) {
        let side = side_rows(diff, rows, new_side);
        let lang = lang_for(path);
        let texts = [diff.old.clone(), diff.new.clone()];
        self.input.update(cx, |input, cx| {
            if input.model.text().is_empty() {
                input.model = TextModel::with_text(&side.text);
            } else {
                let now = input.now(cx);
                input.model.replace_minimal(&side.text, now);
                input.model.forget_history();
            }
            let mut parses = Vec::new();
            let sources = [0, 1].map(|index| {
                let text: Arc<str> = Arc::from(texts[index].as_str());
                let mut syntax = lang.and_then(|lang| Syntax::new(lang).ok());
                // Drawn at once; a parse that runs out of time ends on a background thread and
                // draws the side again (A5).
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
            if let Some(code) = input.code.as_mut() {
                code.lang = lang;
                code.colors += 1;
                if let Some(diff) = code.diff.as_mut() {
                    diff.rows = side;
                    diff.sources = sources;
                    diff._parses = parses;
                }
            }
            input.sync_code(cx);
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

    pub fn text_box(&self) -> TextBox {
        TextBox::of_input(&self.input)
    }

    #[cfg(test)]
    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }
}

#[cfg(test)]
#[path = "diff_side_tests.rs"]
mod tests;
