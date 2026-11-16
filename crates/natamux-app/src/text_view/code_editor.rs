//! What the editor tab's code view has more (plan 6.4, S4.3c): line numbers, sticky rows, long
//! lines that wrap only where the file's kind wraps by default, and the code view's side of
//! `BlameTarget`: the labels at the start of their rows, drawn in the hint style as Zed's inlays
//! were, and the right-click menu of a row (a diff side shares both, `diff_side.rs`).

use gpui::{
    AnyElement, App, DismissEvent, Entity, Focusable, IntoElement, MouseDownEvent, ParentElement,
    Pixels, Point, Subscription, TextRun, Window, anchored, deferred,
};
use theme::ActiveTheme;
use ui::editor_menu::ContextMenu;

use super::code_runs::input_edit;
use super::code_view::CodeView;
use super::text_input::TextInput;
use crate::files::outline_rules::OutlineRow;
use crate::git::change_rules::wraps_by_default;
use crate::git::code_target::{BlameTarget, FindTarget, RowMenu};
use crate::text::model::Splice;

/// The labels of a text, each at its offset in the text now. Each moves with the text as a
/// right-bias anchor does, so it stays at the start of its line when lines come or go above it.
#[derive(Default)]
pub(super) struct Labels {
    /// Each label at the start of its row, in text order.
    spots: Vec<(usize, String)>,
    /// The text the spots are of; kept only while there are labels.
    text: String,
}

impl Labels {
    /// New labels for `text`, in place of the ones before: each at the start of its row; a row
    /// past the end has none.
    pub fn set(&mut self, rows: Vec<(u32, String)>, text: &str) {
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        self.spots = rows
            .into_iter()
            .filter_map(|(row, label)| Some((*starts.get(row as usize)?, label)))
            .collect();
        self.spots.sort_by_key(|(at, _)| *at);
        self.text = if self.spots.is_empty() {
            String::new()
        } else {
            text.to_owned()
        };
    }

    pub fn is_empty(&self) -> bool {
        self.spots.is_empty()
    }

    /// Moves the labels with `edits`, the replaces that turned the text before into `text`, in
    /// the order they were made. Without them (a whole new text) one edit is guessed from the two
    /// texts; blame is asked again then anyway.
    pub fn follow(&mut self, text: &str, edits: Option<&[Splice]>) {
        if self.spots.is_empty() {
            return;
        }
        match edits {
            Some(edits) => edits.iter().for_each(|edit| self.shift(edit)),
            None if self.text != text => {
                let edit = input_edit(&self.text, text);
                self.shift(&Splice {
                    start: edit.start_byte,
                    removed: edit.old_end_byte - edit.start_byte,
                    inserted: edit.new_end_byte - edit.start_byte,
                });
            }
            None => {}
        }
        self.text = text.to_owned();
    }

    /// Moves the labels with one replace, as right-bias anchors: text put in at a label goes
    /// before it, and a label in a removed part goes to the end of what came in its place.
    fn shift(&mut self, edit: &Splice) {
        let end = edit.start + edit.removed;
        for (spot, _) in &mut self.spots {
            if *spot >= end {
                *spot = *spot - edit.removed + edit.inserted;
            } else if *spot >= edit.start {
                *spot = edit.start + edit.inserted;
            }
        }
    }

    pub fn total(&self) -> usize {
        self.spots.iter().map(|(_, label)| label.len()).sum()
    }

    /// `text` with the labels in.
    pub fn insert(&self, text: &str) -> String {
        let mut shown = String::with_capacity(text.len() + self.total());
        let mut at = 0;
        for (spot, label) in &self.spots {
            shown.push_str(&text[at..*spot]);
            shown.push_str(label);
            at = *spot;
        }
        shown.push_str(&text[at..]);
        shown
    }

    /// The offset in the shown text of `at`: a label at a row start comes before the caret there.
    pub fn shown(&self, at: usize) -> usize {
        at + self
            .spots
            .iter()
            .take_while(|(spot, _)| *spot <= at)
            .map(|(_, label)| label.len())
            .sum::<usize>()
    }

    /// The offset in the text of `shown`; one inside a label is its row's start.
    pub fn text_at(&self, shown: usize) -> usize {
        let mut extra = 0;
        for (spot, label) in &self.spots {
            let start = spot + extra;
            if shown < start {
                break;
            }
            if shown < start + label.len() {
                return *spot;
            }
            extra += label.len();
        }
        shown - extra
    }

    /// `runs` of the text with a `label` run in at each label.
    pub fn splice(&self, runs: Vec<TextRun>, label: &TextRun) -> Vec<TextRun> {
        let mut out = Vec::with_capacity(runs.len() + self.spots.len() * 2);
        let mut spots = self.spots.iter().peekable();
        let mut at = 0;
        for run in runs {
            let end = at + run.len;
            let mut start = at;
            while let Some((spot, text)) = spots.next_if(|spot| spot.0 < end) {
                if *spot > start {
                    out.push(TextRun {
                        len: spot - start,
                        ..run.clone()
                    });
                    start = *spot;
                }
                out.push(TextRun {
                    len: text.len(),
                    ..label.clone()
                });
            }
            if end > start {
                out.push(TextRun {
                    len: end - start,
                    ..run.clone()
                });
            }
            at = end;
        }
        // Labels at the end of the text (an empty last row).
        for (_, text) in spots {
            out.push(TextRun {
                len: text.len(),
                ..label.clone()
            });
        }
        out
    }

    /// The ranges of the labels in the shown text.
    #[cfg(test)]
    pub fn shown_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut extra = 0;
        self.spots
            .iter()
            .map(|(spot, label)| {
                let start = spot + extra;
                extra += label.len();
                start..start + label.len()
            })
            .collect()
    }
}

/// A range painted behind the text: a find match (`Some(true)` when active) or another place of
/// the selected text (`None`).
#[cfg(test)]
pub type Mark = (std::ops::Range<usize>, Option<bool>);

/// The right-click menu as it is open.
pub(super) struct OpenMenu {
    menu: Entity<ContextMenu>,
    position: Point<Pixels>,
    _dismiss: Subscription,
}

impl TextInput {
    /// The labels of a code view, when it shows some.
    pub(super) fn labels(&self) -> Option<&Labels> {
        self.code
            .as_ref()
            .map(|code| &code.labels)
            .filter(|labels| !labels.spots.is_empty())
    }

    /// The run of a label: the theme's `hint` style, else the hint colour, as Zed's inlay hints.
    pub(super) fn label_run(&self, base: &TextRun, cx: &App) -> TextRun {
        let style = cx
            .theme()
            .syntax()
            .style_for_name("hint")
            .unwrap_or_default();
        let mut font = base.font.clone();
        if let Some(weight) = style.font_weight {
            font.weight = weight;
        }
        if let Some(slant) = style.font_style {
            font.style = slant;
        }
        TextRun {
            len: 0,
            font,
            color: style.color.unwrap_or(cx.theme().status().hint),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    /// A right press on the text of a code view with a menu: the caret goes there unless it is
    /// inside the selection, then the menu of its row opens at the pointer.
    pub(super) fn right_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(menu) = self.code.as_ref().and_then(|code| code.menu.clone()) else {
            return;
        };
        // As in the Zed editor, the line numbers open no menu, and a pinned row takes no right
        // press.
        if self.in_gutter(event.position) {
            return;
        }
        if let Some(bounds) = self.last_bounds {
            let laid = self.layout(window, cx);
            let rows = self.sticky_paint(&laid, bounds);
            let edges = (bounds.left(), bounds.right());
            let tops = super::code_sticky::tops(&rows);
            if super::code_sticky::sticky_under(&tops, edges, laid.line_height, event.position)
                .is_some()
            {
                cx.stop_propagation();
                return;
            }
        }
        let Some((at, _)) = self.index_at(event.position, window, cx) else {
            return;
        };
        let range = self.model.selection().range();
        if !(range.start <= at && at <= range.end && !range.is_empty()) {
            self.model.move_to(at);
            self.changed(false, cx);
            self.no_click_scroll();
        }
        let row = self.model.text()[..at].matches('\n').count() as u32;
        // A diff side's rows are rows of its own file; a deleted line of an inline diff is none.
        let row = match self.code.as_ref().and_then(|code| code.diff.as_ref()) {
            Some(diff) => diff.file_row(row as usize),
            None => Some(row),
        };
        cx.stop_propagation();
        let Some(menu) = menu(row, window, cx) else {
            return;
        };
        let dismiss = cx.subscribe_in(
            &menu,
            window,
            |input, menu, _: &DismissEvent, window, cx| {
                if menu.focus_handle(cx).contains_focused(window, cx) {
                    window.focus(&input.focus, cx);
                }
                if let Some(code) = input.code.as_mut() {
                    code.open_menu = None;
                }
                cx.notify();
            },
        );
        window.focus(&menu.focus_handle(cx), cx);
        if let Some(code) = self.code.as_mut() {
            code.open_menu = Some(OpenMenu {
                menu,
                position: event.position,
                _dismiss: dismiss,
            });
        }
        cx.notify();
    }

    /// The open right-click menu, over everything at the pointer.
    pub(super) fn menu_element(&self) -> Option<AnyElement> {
        let open = self.code.as_ref()?.open_menu.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(open.position)
                    .anchor(gpui::Anchor::TopLeft)
                    .child(open.menu.clone()),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

impl CodeView {
    /// The editor tab's view of `text`: line numbers, sticky rows, no line of the caret painted,
    /// and long lines wrapped only where the file's kind wraps by default.
    pub fn editor(path: &str, text: &str, window: &mut Window, cx: &mut App) -> Self {
        let view = Self::new(path, text, false, window, cx);
        let wrap = wraps_by_default(path);
        view.input_ref().update(cx, |input, _| {
            if let Some(code) = input.code.as_mut() {
                code.gutter = true;
                code.sticky = true;
                code.wrap = wrap;
            }
        });
        view
    }

    pub fn wraps(&self, cx: &App) -> bool {
        let input = self.input_ref().read(cx);
        input.code.as_ref().is_none_or(|code| code.wrap)
    }

    /// The text is parsed in its language, so its colours show; a `--shot` waits for it. False
    /// for a file with no language, as the old editor's check was.
    pub fn parsed(&self, cx: &App) -> bool {
        let input = self.input_ref().read(cx);
        let syntax = input.code.as_ref().and_then(|code| code.syntax.as_ref());
        syntax.is_some_and(|syntax| syntax.tree().is_some() && syntax.is_done())
    }

    /// The items of Go to symbol, in text order; none until the text has a tree.
    pub fn outline_rows(&self, cx: &App) -> Vec<OutlineRow> {
        let input = self.input_ref().read(cx);
        let text = input.model.text();
        input
            .cached_outline()
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

    /// A new text from disk, with the fewest edits so the caret and the scroll stay. The undo
    /// history goes with it (T-0316): an undo would edit the disk text back toward the old one.
    pub fn set_disk_text(&self, text: &str, cx: &mut App) {
        self.input_ref().update(cx, |input, cx| {
            input.replace_minimal(text, cx);
            input.model.forget_history();
        });
    }

    /// Changes the text into `text` with the fewest edits, as one undo step.
    pub fn replace_minimal(&self, text: &str, cx: &mut App) {
        self.input_ref()
            .update(cx, |input, cx| input.replace_minimal(text, cx));
    }

    /// The caret goes to `at` with its row in the middle of the view, and the view takes the
    /// focus.
    pub fn go_to_offset(&self, at: usize, window: &mut Window, cx: &mut App) {
        self.select(at..at, window, cx);
        let focus = self.input_ref().read(cx).focus.clone();
        window.focus(&focus, cx);
    }

    /// Scrolls so `top` (0-based, may be part of a row) is the top row, for the `--shot` states.
    pub fn scroll_to_row(&self, top: f64, window: &mut Window, cx: &mut App) {
        self.input_ref().update(cx, |input, cx| {
            let laid = input.layout(window, cx);
            input.scroll.y = laid.line_height * top as f32;
            input.autoscroll = false;
            cx.notify();
        });
    }

    /// The text with the blame labels in, as it shows.
    #[cfg(test)]
    pub fn shown_text(&self, cx: &App) -> String {
        let input = self.input_ref().read(cx);
        match input.labels() {
            Some(labels) => labels.insert(input.model.text()),
            None => input.model.text().to_owned(),
        }
    }

    /// What is painted behind the text past the selection: each find match (`Some(true)` for
    /// the active one) and each other place of the selected text (`None`); and the selection.
    #[cfg(test)]
    pub fn marks(&self, cx: &App) -> (Vec<Mark>, std::ops::Range<usize>) {
        let input = self.input_ref().read(cx);
        let selection = input.model.selection().range();
        let Some(code) = input.code.as_ref() else {
            return (Vec::new(), selection);
        };
        let mut marks: Vec<_> = code
            .other_occurrences(selection.clone())
            .map(|range| (range.clone(), None))
            .collect();
        if code.search_version == input.model.version() {
            marks.extend(
                code.search
                    .iter()
                    .enumerate()
                    .map(|(index, range)| (range.clone(), Some(code.search_active == Some(index)))),
            );
        }
        (marks, selection)
    }

    /// The header's Wrap long lines toggle.
    pub fn set_wrap(&self, wrap: bool, cx: &mut App) {
        self.input_ref().update(cx, |input, cx| {
            if let Some(code) = input.code.as_mut() {
                code.wrap = wrap;
            }
            input.scroll.x = gpui::px(0.);
            cx.notify();
        });
    }
}

impl BlameTarget for CodeView {
    fn lines(&self, cx: &App) -> u32 {
        self.input_ref().read(cx).text().split('\n').count() as u32
    }

    fn set_menu(&self, menu: RowMenu, cx: &mut App) {
        self.input_ref().update(cx, |input, _| {
            if let Some(code) = input.code.as_mut() {
                code.menu = Some(menu);
            }
        });
    }

    fn set_labels(&self, labels: Vec<(u32, String)>, cx: &mut App) {
        self.input_ref().update(cx, |input, cx| {
            let text = input.model.text();
            if let Some(code) = input.code.as_mut() {
                code.labels.set(labels, text);
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
#[path = "code_editor_tests.rs"]
mod tests;
