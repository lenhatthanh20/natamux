//! The text diff of the Changes tab (docs/ui-spec.md 4.24, `MonacoDiff.tsx` and the whole-file
//! body of `FileBody.tsx`), on the own diff model (`diff_model.rs`) and diff sides
//! (`text_view/diff_side.rs`, plan 6.5): read-only code views over the working-tree text, with
//! the last commit's text as the diff base.
//!
//! Inline is one side with the deleted lines shown between the new ones. Side by side is two
//! sides: the last commit at the left (its deleted lines marked), the working tree at the right
//! (its added lines marked). Empty rows keep the two sides level, and a scroll of one side
//! scrolls the other. "Collapse unchanged lines" shows 3 lines around each change. The buttons of
//! a change and the review comments are drawn over the sides, where their rows are.

use gpui::{
    EventEmitter, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Subscription,
};
use ui::prelude::*;

use super::change_rules::{LineChange, Side};
use super::diff_model::{self, FileDiff, Row, Status, inline_rows, word_chars};
use super::diff_rows::{self, SideRows};
use super::diff_rules::{DiffRefresh, HunkRows, diff_refresh, line_change};
use super::languages::lang_for;
use super::review_blocks::ReviewInput;
use super::whitespace_rules::diff_base;
use crate::text_view::diff_side::{DiffSide, Place};
use crate::text_view::text_input::actions::ToggleSelectedDiffHunks;
use natamux_proto::workspaces::review_comments::ReviewComment;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffOptions {
    pub side_by_side: bool,
    pub collapse: bool,
    pub wrap: bool,
    pub ignore_whitespace: bool,
}

/// What the Changes tab follows: the changes were worked out (the toolbar's counts and
/// navigator), the cursor or the selection moved (the comment box's label), or a revert arrow
/// was clicked (hunk `index` of the read `version`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffEvent {
    Ready,
    CursorMoved,
    RevertHunk { index: usize, version: u64 },
}

/// The left side is 0, the right side (or the only one, inline) is 1.
pub(super) const LEFT: usize = 0;
pub(super) const RIGHT: usize = 1;

pub(super) struct Sides {
    /// The working tree at the right, or alone when inline.
    pub(super) right: DiffSide,
    pub(super) left: Option<DiffSide>,
    /// The file the sides show.
    pub(super) path: Option<String>,
    /// The comments each side draws (left, right), with the block of its rows that holds each.
    pub(super) comments: [Vec<(usize, ReviewComment)>; 2],
    /// The selection each side had at the last look, so a move is told once.
    seen: [(usize, usize); 2],
    _subscriptions: Vec<Subscription>,
}

impl Sides {
    pub(super) fn get(&self, index: usize) -> Option<&DiffSide> {
        if index == LEFT {
            self.left.as_ref()
        } else {
            Some(&self.right)
        }
    }
}

pub struct DiffView {
    key: Option<String>,
    pub(super) version: u64,
    /// The read whose texts `hunks` were worked out from.
    pub(super) hunks_for: Option<u64>,
    pub(super) movable: bool,
    pub(super) moving: bool,
    options: Option<DiffOptions>,
    /// The texts of the shown read: the last commit's (None for a new file) and the working
    /// tree's.
    texts: Option<(Option<String>, String)>,
    pub(super) diff: Option<FileDiff>,
    pub(super) hunks: Vec<HunkRows>,
    path: Option<String>,
    pub(super) review: ReviewInput,
    pub(super) sides: Option<Sides>,
    /// The changes of an inline diff whose deleted lines the user closed (Mod+').
    pub(super) closed: Vec<usize>,
    /// The change under the pointer on each side; its buttons show too.
    pub(super) hovered: [Option<usize>; 2],
}

impl EventEmitter<DiffEvent> for DiffView {}

impl DiffView {
    pub fn new() -> Self {
        DiffView {
            key: None,
            version: 0,
            hunks_for: None,
            movable: false,
            moving: false,
            options: None,
            texts: None,
            diff: None,
            hunks: Vec::new(),
            path: None,
            review: ReviewInput::default(),
            sides: None,
            closed: Vec::new(),
            hovered: [None, None],
        }
    }

    /// Shows `modified` against `original` (None: the file is new). `key` names the task and the
    /// file, `version` the read of its texts: a newer read of the same file changes the texts
    /// inside the sides, so the cursor, the selection and the scroll stay. Cheap when nothing
    /// changed, so a render may call it.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        key: &str,
        path: &str,
        version: u64,
        original: Option<&str>,
        modified: &str,
        options: DiffOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_file = self.key.as_deref() == Some(key);
        let same_read = same_file && self.version == version;
        // Ignore whitespace changes the diff's base, so the texts are diffed again.
        let ignore = options.ignore_whitespace;
        let same_base = self.options.map(|o| o.ignore_whitespace) == Some(ignore);
        match diff_refresh(same_file, same_read, self.options == Some(options)) {
            DiffRefresh::Keep => {}
            DiffRefresh::Update if self.sides.is_some() => {
                self.version = version;
                self.texts = Some((original.map(str::to_owned), modified.to_owned()));
                self.work_out();
                // The changes come back open, as Zed's editor opened them all again.
                self.closed.clear();
                self.refresh_rows(cx);
                cx.emit(DiffEvent::Ready);
                cx.notify();
            }
            _ if same_read && same_base => {
                self.options = Some(options);
                self.build(window, cx);
            }
            _ => self.load(
                key,
                path,
                version,
                (original, modified),
                options,
                window,
                cx,
            ),
        }
    }

    /// New sides for `texts` (original, modified); the same file keeps its scroll position.
    #[allow(clippy::too_many_arguments)]
    fn load(
        &mut self,
        key: &str,
        path: &str,
        version: u64,
        (original, modified): (Option<&str>, &str),
        options: DiffOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_file = self.key.as_deref() == Some(key);
        let scroll = same_file
            .then_some(self.sides.as_ref())
            .flatten()
            .map(|sides| sides.right.scroll(cx));
        self.key = Some(key.to_owned());
        self.path = Some(path.to_owned());
        self.version = version;
        self.options = Some(options);
        self.texts = Some((original.map(str::to_owned), modified.to_owned()));
        self.work_out();
        self.build(window, cx);
        if let (Some(scroll), Some(sides)) = (scroll, self.sides.as_ref()) {
            sides.right.set_scroll(scroll, cx);
        }
    }

    /// The hunks and the changed words of the shown texts.
    fn work_out(&mut self) {
        let (Some((original, modified)), Some(options)) = (&self.texts, self.options) else {
            return;
        };
        let path = self.path.as_deref().unwrap_or_default();
        let base = diff_base(original.as_deref(), modified, options.ignore_whitespace);
        let diff = diff_model::diff(base.as_deref(), modified, word_chars(path));
        self.hunks = diff.hunk_rows();
        self.diff = Some(diff);
        self.hunks_for = Some(self.version);
    }

    /// Another body shows: drop the sides, so a later show starts fresh.
    pub fn clear(&mut self) {
        *self = DiffView::new();
    }

    /// The diff of the shown texts is worked out and drawn.
    pub fn ready(&self) -> bool {
        self.sides.is_some()
    }

    /// `path` is drawn, and the working tree's file is parsed in its language. False for a file
    /// with no language, as the old editor's check was.
    pub fn parsed(&self, path: &str, cx: &App) -> bool {
        self.shown(path)
            && lang_for(path).is_some()
            && self
                .sides
                .as_ref()
                .is_some_and(|sides| sides.right.new_parsed(cx))
    }

    /// The last commit's file is parsed too: the left side draws its lines from it.
    pub fn base_parsed(&self, cx: &App) -> bool {
        let Some(sides) = &self.sides else {
            return false;
        };
        let path = sides.path.as_deref().unwrap_or_default();
        lang_for(path).is_some()
            && sides.right.parsed(cx)
            && sides.left.as_ref().is_none_or(|left| left.parsed(cx))
    }

    /// `path` is drawn: its diff is worked out and the sides are built.
    pub fn shown(&self, path: &str) -> bool {
        self.path.as_deref() == Some(path) && self.ready()
    }

    /// The changes in Monaco's shape, for the counts and the navigator.
    pub fn changes(&self) -> Vec<LineChange> {
        self.hunks.iter().map(line_change).collect()
    }

    /// New sides for the options, every change open.
    fn build(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(_), Some(options)) = (&self.diff, self.options) else {
            return;
        };
        self.closed.clear();
        self.hovered = [None, None];
        let right = DiffSide::new(options.wrap, window, cx);
        let left = options
            .side_by_side
            .then(|| DiffSide::new(options.wrap, window, cx));
        if let Some(left) = &left {
            left.pair(&right, cx);
        }
        let mut subscriptions = vec![Self::watch(&right, RIGHT, cx)];
        if let Some(left) = &left {
            subscriptions.push(Self::watch(left, LEFT, cx));
        }
        self.sides = Some(Sides {
            right,
            left,
            path: self.path.clone(),
            comments: [Vec::new(), Vec::new()],
            seen: [(0, 0); 2],
            _subscriptions: subscriptions,
        });
        self.refresh_rows(cx);
        // New sides: the owner hooks them up again (blame menu), also after an option change.
        cx.emit(DiffEvent::Ready);
        cx.notify();
    }

    /// A side's selection moved: the Changes tab hears it, and the buttons follow the caret.
    fn watch(side: &DiffSide, index: usize, cx: &mut Context<Self>) -> Subscription {
        side.observe(cx, move |view, cx| {
            let Some(sides) = view.sides.as_mut() else {
                return;
            };
            let Some(now) = sides.get(index).map(|side| side.selection(cx)) else {
                return;
            };
            if sides.seen[index] != now {
                sides.seen[index] = now;
                cx.emit(DiffEvent::CursorMoved);
                cx.notify();
            }
        })
    }

    /// The rows of each side for the shown diff, the options, the closed changes and the
    /// comments, put into the sides (the same sides: the caret and the scroll stay).
    pub(super) fn refresh_rows(&mut self, cx: &mut Context<Self>) {
        let Some(options) = self.options else {
            return;
        };
        let (Some(diff), Some(sides)) = (self.diff.as_ref(), self.sides.as_ref()) else {
            return;
        };
        let mut shown = self.side_rows(diff, options, sides.left.is_some());
        let comments = self.comment_blocks(&mut shown);
        let path = self.path.clone().unwrap_or_default();
        let targets: Vec<(DiffSide, SideRows, bool)> = shown
            .into_iter()
            .filter_map(|(index, rows)| Some((sides.get(index)?.clone(), rows, index == RIGHT)))
            .collect();
        for (side, rows, new_side) in targets {
            side.show_rows(&path, diff, rows, new_side, cx);
        }
        if let Some(sides) = self.sides.as_mut() {
            sides.comments = comments;
        }
    }

    /// What each side shows: (side, rows), the left side first.
    fn side_rows(
        &self,
        diff: &FileDiff,
        options: DiffOptions,
        two_sides: bool,
    ) -> Vec<(usize, SideRows)> {
        let (old_rows, new_rows) = (diff.old_rows(), diff.new_rows());
        let hunks = &self.hunks;
        if two_sides {
            let rows =
                |side| diff_model::side_rows(hunks, side, old_rows, new_rows, options.collapse);
            return vec![
                (LEFT, diff_rows::side_rows(diff, &rows(Side::Old), false)),
                (RIGHT, diff_rows::side_rows(diff, &rows(Side::New), true)),
            ];
        }
        let closed = |row: u32| {
            self.closed
                .iter()
                .any(|&index| hunks.get(index).is_some_and(|hunk| hunk.old.contains(&row)))
        };
        let rows: Vec<Row> = inline_rows(hunks, new_rows, options.collapse)
            .into_iter()
            .filter(|row| !matches!(row, Row::Deleted { row } if closed(*row)))
            .collect();
        let mut side = diff_rows::side_rows(diff, &rows, true);
        side.close(&self.closed);
        vec![(RIGHT, side)]
    }

    /// Mod+' in an inline diff: the change at the caret closes or opens its deleted lines. Side
    /// by side keeps every change open.
    fn toggle_hunk(&mut self, _: &ToggleSelectedDiffHunks, _: &mut Window, cx: &mut Context<Self>) {
        let Some(sides) = &self.sides else {
            return;
        };
        if sides.left.is_some() {
            return;
        }
        let head = sides.right.selection(cx).1;
        // A closed hunk's lines keep their hunk, so the caret on one opens it again.
        let at = sides.right.rows(cx, |rows| {
            let line = rows.lines.get(rows.line_at(head))?;
            Some((line.hunk, line.old, line.row))
        });
        let Some((hunk, old, row)) = at.flatten() else {
            return;
        };
        // A closed change of deleted lines only has no line left: the caret is on the line below.
        let hunk = hunk.or_else(|| {
            (!old)
                .then(|| {
                    self.hunks
                        .iter()
                        .position(|hunk| hunk.new.is_empty() && hunk.new.start == row)
                })
                .flatten()
        });
        if let Some(hunk) = hunk {
            self.toggle(hunk, cx);
        }
    }

    /// Change `hunk` of an inline diff closes, or opens again.
    fn toggle(&mut self, hunk: usize, cx: &mut Context<Self>) {
        match self.closed.iter().position(|&closed| closed == hunk) {
            Some(at) => {
                self.closed.remove(at);
            }
            None => self.closed.push(hunk),
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    /// A press on the strip of a change in the gutter, or on the marker of a closed change of
    /// deleted lines only (the side left it to the view): an inline diff closes or opens the
    /// change, as Zed's editor did (T-MOUSE-1); side by side keeps it.
    fn strip_press(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sides) = &self.sides else {
            return;
        };
        if sides.left.is_some() {
            return;
        }
        let side = sides.right.clone();
        let Some(place) = side.place(window, cx) else {
            return;
        };
        let at = position - place.origin;
        let on_strip = at.x >= px(0.) && at.x < place.strip();
        let hunk = side
            .rows(cx, |rows| {
                // Unchanged lines have no hunk, but those of a closed change.
                let on_line = place
                    .line_at(at.y)
                    .filter(|_| on_strip)
                    .and_then(|line| rows.lines.get(line)?.hunk);
                on_line.or_else(|| place.closed_marker_at(rows, at))
            })
            .flatten();
        if let Some(hunk) = hunk {
            self.toggle(hunk, cx);
        }
    }

    /// The pointer moved over side `index`: the change under it, over the text, shows its
    /// buttons.
    fn hover(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(side) = self
            .sides
            .as_ref()
            .and_then(|sides| sides.get(index))
            .cloned()
        else {
            return;
        };
        let hunk = side.place(window, cx).and_then(|place| {
            let at = position - place.origin;
            let over_text = at.x >= place.gutter()
                && at.x < place.size.width
                && at.y >= px(0.)
                && at.y < place.size.height;
            let line = place.line_at(at.y).filter(|_| over_text)?;
            side.rows(cx, |rows| {
                let line = rows.lines.get(line)?;
                line.hunk.filter(|_| line.status != Status::Unchanged)
            })
            .flatten()
        });
        self.set_hovered(index, hunk, cx);
    }

    fn set_hovered(&mut self, index: usize, hunk: Option<usize>, cx: &mut Context<Self>) {
        if self.hovered[index] != hunk {
            self.hovered[index] = hunk;
            cx.notify();
        }
    }

    /// A side with what is drawn over it: the review comments and the buttons of its changes.
    fn side_element(
        &mut self,
        side: &DiffSide,
        index: usize,
        alone: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut over: Vec<AnyElement> = Vec::new();
        let mut rem = window.rem_size();
        match side.place(window, cx) {
            Some(place) => {
                rem = place.rem_size();
                let pinned = side
                    .rows(cx, |rows| place.pins_header(rows))
                    .unwrap_or(false);
                over.extend(self.comment_elements(index, &place, cx));
                over.extend(self.hunk_buttons(side, index, &place, pinned, cx));
                if pinned {
                    over.push(super::diff_header::pinned_header(&place, cx));
                }
            }
            // Not drawn yet: what lies over it needs its rows, so draw once more.
            None => cx.on_next_frame(window, |_, _, cx| cx.notify()),
        }
        div()
            .id(("diff-side", index))
            .relative()
            .h_full()
            .map(|this| {
                if alone {
                    this.size_full()
                } else {
                    this.flex_1().min_w_0()
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    view.strip_press(event.position, window, cx)
                }),
            )
            .on_mouse_move(
                cx.listener(move |view, event: &MouseMoveEvent, window, cx| {
                    view.hover(index, event.position, window, cx)
                }),
            )
            .on_hover(cx.listener(move |view, hovered: &bool, window, cx| {
                // The change's buttons block the mouse, so the side is not hovered over them:
                // clearing would hide them, show them again, and flicker (T-0345).
                if !*hovered {
                    view.hover(index, window.mouse_position(), window, cx);
                }
            }))
            .child(side.text_box())
            .child(
                ui::WithRemSize::new(rem)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .overflow_hidden()
                    .children(over),
            )
            .into_any_element()
    }

    /// The buttons of the change the caret is in and of the one under the pointer, at the right
    /// of its first row (or of the top row, while that row is scrolled above it or under the
    /// `pinned` header), as Zed's editor placed them: 10 px left of the scrollbar's room.
    fn hunk_buttons(
        &self,
        side: &DiffSide,
        index: usize,
        place: &Place,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let top_edge = if pinned {
            super::diff_header::header_bottom(place)
        } else {
            px(0.)
        };
        let head = side.selection(cx).1;
        let caret = side
            .rows(cx, |rows| {
                let line = rows.lines.get(rows.line_at(head))?;
                line.hunk.filter(|_| line.status != Status::Unchanged)
            })
            .flatten();
        let mut active = vec![];
        for hunk in [self.hovered[index], caret].into_iter().flatten() {
            if !active.contains(&hunk) {
                active.push(hunk);
            }
        }
        let line_height = place.line_height();
        let view = cx.weak_entity();
        let mut out = Vec::new();
        for hunk in active {
            let span = side
                .rows(cx, |rows| {
                    let changed = |line: &&diff_rows::SideLine| {
                        line.hunk == Some(hunk) && line.status != Status::Unchanged
                    };
                    let first = rows.lines.iter().position(|line| changed(&line))?;
                    let count = rows.lines[first..]
                        .iter()
                        .take_while(|line| changed(line))
                        .count();
                    let line = rows.lines[first];
                    Some((first, first + count - 1, line.old, line.row))
                })
                .flatten();
            let Some((first, last, old, row)) = span else {
                continue;
            };
            let (Some((top, _)), Some((last_top, last_rows))) =
                (place.line(first), place.line(last))
            else {
                continue;
            };
            let bottom = last_top + line_height * last_rows as f32;
            if bottom <= px(0.) || top >= place.size.height {
                continue;
            }
            let y = if top >= top_edge {
                top
            } else {
                top_edge.min(bottom - line_height)
            };
            let file_side = if old { Side::Old } else { Side::New };
            let Some(arrow) = self.arrow_at(file_side, row) else {
                continue;
            };
            let buttons = super::diff_hunk_controls::hunk_buttons(
                view.clone(),
                index == RIGHT,
                arrow,
                first,
                line_height,
                cx,
            );
            out.push(
                div()
                    .absolute()
                    .top(y)
                    .right(place.room + px(10.))
                    .child(buttons)
                    .into_any_element(),
            );
        }
        out
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(sides) = &self.sides else {
            return div().size_full().into_any_element();
        };
        let (right, left) = (sides.right.clone(), sides.left.clone());
        let border = cx.theme().colors().border;
        match left {
            Some(left) => {
                let left = self.side_element(&left, LEFT, false, window, cx);
                let right = self.side_element(&right, RIGHT, false, window, cx);
                h_flex()
                    .key_context("NatamuxDiff")
                    .size_full()
                    .on_action(cx.listener(Self::toggle_hunk))
                    .child(left)
                    .child(div().w_px().h_full().bg(border))
                    .child(right)
                    .into_any_element()
            }
            None => {
                let right = self.side_element(&right, RIGHT, true, window, cx);
                div()
                    .key_context("NatamuxDiff")
                    .size_full()
                    .on_action(cx.listener(Self::toggle_hunk))
                    .child(right)
                    .into_any_element()
            }
        }
    }
}

#[cfg(test)]
#[path = "diff_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "diff_mouse_tests.rs"]
mod mouse_tests;
