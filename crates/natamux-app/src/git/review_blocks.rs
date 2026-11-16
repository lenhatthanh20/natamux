//! The review comments inside the diff: rows under the last line of each comment, with a fixed
//! height (one row per body line, no wrap), and in side by side the empty rows of the same height
//! at the matching row of the other side, so both stay level. The diff view draws each comment
//! over its rows.

use gpui::{Pixels, WeakEntity};
use natamux_proto::workspaces::review_comments::{CommentState, ReviewComment};
use ui::prelude::*;

use super::changes_view::ChangesView;
use super::diff_rows::{BlockKind, Extra, SideRows};
use super::diff_view::{DiffView, LEFT, RIGHT};
use super::review_rules::{
    MORE_TEXT, Spot, actions, block_rows, body_lines, has_block, placement, state_label,
};
use crate::components::button_style::quiet_button;
use crate::text_view::diff_side::Place;

/// The room a comment is laid out in: more than any comment needs.
const FIXED_ROOM: Pixels = px(8192.);

/// The comments the diff draws, and the view their buttons call.
#[derive(Default)]
pub(super) struct ReviewInput {
    pub(super) comments: Vec<ReviewComment>,
    owner: Option<WeakEntity<ChangesView>>,
}

impl DiffView {
    /// The comments to draw (the visible ones of the task). Their rows follow at once when the
    /// sides are there; else the next build puts them in.
    pub fn set_comments(
        &mut self,
        comments: Vec<ReviewComment>,
        owner: WeakEntity<ChangesView>,
        cx: &mut Context<Self>,
    ) {
        if self.review.comments == comments {
            return;
        }
        self.review.comments = comments;
        self.review.owner = Some(owner);
        if self.sides.is_some() {
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    /// The sides show this file.
    pub fn shows(&self, path: &str) -> bool {
        self.sides
            .as_ref()
            .is_some_and(|sides| sides.path.as_deref() == Some(path))
    }

    /// Puts the rows of each comment of the shown file into the rows of the sides (`shown`,
    /// (side, rows)), and gives for each side the comments it draws with the block of each. A
    /// comment whose row does not show (collapsed) has no rows.
    pub(super) fn comment_blocks(
        &self,
        shown: &mut [(usize, SideRows)],
    ) -> [Vec<(usize, ReviewComment)>; 2] {
        let mut drawn: [Vec<(usize, ReviewComment)>; 2] = [Vec::new(), Vec::new()];
        let (Some(path), Some(diff)) = (
            self.sides.as_ref().and_then(|sides| sides.path.clone()),
            self.diff.as_ref(),
        ) else {
            return drawn;
        };
        if self.review.owner.is_none() {
            return drawn;
        }
        let two_sides = shown.len() == 2;
        // The last row of each file: the last commit's (left) and the working tree's.
        let last = [
            diff.old_rows().saturating_sub(1),
            diff.new_rows().saturating_sub(1),
        ];
        let rows_of = |side: usize| {
            shown
                .iter()
                .find(|(index, _)| *index == side)
                .map(|(_, rows)| rows)
        };
        let mut extra: [Vec<(Extra, Option<ReviewComment>)>; 2] = [Vec::new(), Vec::new()];
        for comment in self
            .review
            .comments
            .iter()
            .filter(|comment| comment.path == path && has_block(comment))
        {
            let place = placement(comment, &self.hunks, two_sides);
            let mine = if two_sides && place.left { LEFT } else { RIGHT };
            let other = two_sides.then_some(if mine == LEFT { RIGHT } else { LEFT });
            let count = block_rows(&comment.body) as usize;
            let own = rows_of(mine).and_then(|rows| {
                extra_at(
                    rows,
                    mine,
                    last[mine],
                    place.spot,
                    count,
                    BlockKind::Comment,
                )
            });
            let matching = match (other, place.other) {
                (Some(other), Some(spot)) => rows_of(other)
                    .and_then(|rows| {
                        extra_at(rows, other, last[other], spot, count, BlockKind::Spacer)
                    })
                    .map(|more| (other, more)),
                _ => None,
            };
            let Some(own) = own else {
                continue;
            };
            if other.is_some() && matching.is_none() {
                continue;
            }
            extra[mine].push((own, Some(comment.clone())));
            if let Some((other, more)) = matching {
                extra[other].push((more, None));
            }
        }
        for (index, rows) in shown.iter_mut() {
            let list = &extra[*index];
            let blocks: Vec<Extra> = list.iter().map(|(more, _)| *more).collect();
            let placed = rows.add_blocks(&blocks);
            for ((_, comment), block) in list.iter().zip(placed) {
                if let Some(comment) = comment {
                    drawn[*index].push((block, comment.clone()));
                }
            }
        }
        drawn
    }

    /// The comments of side `index`, each over its rows.
    pub(super) fn comment_elements(
        &self,
        index: usize,
        place: &Place,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (Some(sides), Some(owner)) = (self.sides.as_ref(), self.review.owner.as_ref()) else {
            return Vec::new();
        };
        let Some(side) = sides.get(index) else {
            return Vec::new();
        };
        let line_height = place.line_height();
        let mut out = Vec::new();
        for (block, comment) in &sides.comments[index] {
            let Some((line, offset)) = side.rows(cx, |rows| rows.block_offset(*block)).flatten()
            else {
                continue;
            };
            let top = place.block_top(line, offset);
            let height = line_height * block_rows(&comment.body) as f32;
            if top + height <= px(0.) || top >= place.size.height {
                continue;
            }
            // From the side's left edge, over the gutter, as Zed's editor drew a fixed block; it
            // moves with a scroll to the right. Zed laid a fixed block out with no width limit,
            // so it is as wide as its text even past the side's edge (cut there).
            out.push(
                h_flex()
                    .absolute()
                    .top(top)
                    .left(-place.scroll().x)
                    .w(FIXED_ROOM)
                    .items_start()
                    .child(render_block(comment, owner, line_height, cx))
                    .into_any_element(),
            );
        }
        out
    }
}

/// A spot as rows put in on side `side` (the left side shows the last commit's file); None when
/// its row does not show.
fn extra_at(
    rows: &SideRows,
    side: usize,
    last: u32,
    spot: Spot,
    count: usize,
    kind: BlockKind,
) -> Option<Extra> {
    let (row, below) = match spot {
        Spot::Above(row) if row > last => (last, true),
        Spot::Above(row) => (row, false),
        Spot::Below(row) => (row.min(last), true),
    };
    let line = rows.line_of(side == LEFT, row)?;
    Some(Extra {
        line: if below { line + 1 } else { line },
        rows: count,
        kind,
        early: below,
    })
}

fn state_color(state: CommentState) -> Color {
    match state {
        CommentState::Draft => Color::Warning,
        CommentState::Sent => Color::Info,
        CommentState::Resolved => Color::Success,
    }
}

/// The header (id, state, buttons) and the body lines, each exactly one row high.
fn render_block(
    comment: &ReviewComment,
    owner: &WeakEntity<ChangesView>,
    line: Pixels,
    cx: &App,
) -> AnyElement {
    let (background, accent) = {
        let colors = cx.theme().colors();
        (colors.surface_background, colors.border_variant)
    };
    let buttons: Vec<Button> = actions(comment.state)
        .iter()
        .map(|action| {
            let (owner, id, action) = (owner.clone(), comment.id.clone(), *action);
            quiet_button(
                SharedString::from(format!("review-{}-{}", comment.id, action.label())),
                action.label(),
            )
            .size(ButtonSize::Compact)
            .label_size(LabelSize::XSmall)
            .on_click(move |_, window, cx| {
                owner
                    .update(cx, |view, cx| {
                        view.run_comment_action(action, &id, window, cx)
                    })
                    .ok();
            })
        })
        .collect();
    let header = h_flex()
        .h(line)
        .flex_none()
        .px_2()
        .gap_2()
        .items_center()
        .child(Label::new(comment.id.clone()).size(LabelSize::XSmall))
        .child(
            Label::new(state_label(comment.state))
                .size(LabelSize::XSmall)
                .color(state_color(comment.state)),
        )
        .child(div().flex_1())
        .children(buttons);
    let (shown, cut) = body_lines(&comment.body);
    let text_row = |text: &str, color: Color| {
        h_flex()
            .h(line)
            .flex_none()
            .px_2()
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .child(
                Label::new(text.to_owned())
                    .size(LabelSize::XSmall)
                    .color(color)
                    .truncate(),
            )
    };
    let mut rows: Vec<Div> = shown
        .iter()
        .map(|text| text_row(text, Color::Default))
        .collect();
    if rows.is_empty() {
        rows.push(text_row("", Color::Default));
    }
    if cut {
        rows.push(text_row(MORE_TEXT, Color::Muted));
    }
    v_flex()
        .flex_none()
        .h(line * block_rows(&comment.body) as f32)
        .overflow_hidden()
        .bg(background)
        .border_l_2()
        .border_color(accent)
        .child(header)
        .children(rows)
        .into_any_element()
}
