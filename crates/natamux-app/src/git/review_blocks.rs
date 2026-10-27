//! The review comments inside the diff editors: one block under the last line of each comment,
//! with a fixed height (one row per body line, no wrap), and in side by side the empty rows of
//! the same height at the matching row of the other editor. Zed balances only its own split
//! view, not our two editors, so the heights must be known up front and equal on both sides.

use std::sync::Arc;

use editor::display_map::{
    BlockContext, BlockPlacement, BlockProperties, BlockStyle, CustomBlockId, RenderBlock,
};
use editor::{Anchor, Editor};
use gpui::{Entity, WeakEntity};
use language::Buffer;
use natamux_proto::workspaces::review_comments::{CommentState, ReviewComment};
use text::Point;
use ui::prelude::*;

use super::changes_view::ChangesView;
use super::diff_rules::HunkRows;
use super::diff_view::DiffView;
use super::review_rules::{
    MORE_TEXT, Spot, actions, block_rows, body_lines, has_block, placement, state_label,
};
use crate::components::button_style::quiet_button;

/// The comments the diff draws, and the view their buttons call.
#[derive(Default)]
pub(super) struct ReviewInput {
    pub(super) comments: Vec<ReviewComment>,
    owner: Option<WeakEntity<ChangesView>>,
}

type Placed = Vec<(Entity<Editor>, CustomBlockId)>;

/// One editor with the buffer it shows.
struct Pane {
    editor: Entity<Editor>,
    buffer: Entity<Buffer>,
}

impl DiffView {
    /// The comments to draw (the visible ones of the task). The blocks follow at once when the
    /// editors are there; else the next `build` places them.
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
            self.remove_comment_blocks(cx);
            self.place_comments(cx);
        }
    }

    /// The editors show this file.
    pub fn shows(&self, path: &str) -> bool {
        self.sides
            .as_ref()
            .is_some_and(|sides| sides.path.as_deref() == Some(path))
    }

    pub(super) fn remove_comment_blocks(&mut self, cx: &mut Context<Self>) {
        let Some(sides) = self.sides.as_mut() else {
            return;
        };
        for (editor, id) in std::mem::take(&mut sides.comment_blocks) {
            editor.update(cx, |editor, cx| {
                editor.remove_blocks([id].into_iter().collect(), None, cx)
            });
        }
    }

    /// A block for each comment of the shown file that has its lines in the excerpts.
    pub(super) fn place_comments(&mut self, cx: &mut Context<Self>) {
        let (Some(buffer), Some(diff), Some(owner)) = (
            self.buffer.clone(),
            self.diff.clone(),
            self.review.owner.clone(),
        ) else {
            return;
        };
        let Some(sides) = self.sides.as_ref() else {
            return;
        };
        let Some(path) = sides.path.clone() else {
            return;
        };
        let base = diff.read(cx).base_text_buffer().clone();
        let right = Pane {
            editor: sides.right.clone(),
            buffer,
        };
        let left = sides.left.clone().map(|editor| Pane {
            editor,
            buffer: base,
        });
        let mut placed: Placed = Vec::new();
        for comment in self
            .review
            .comments
            .iter()
            .filter(|comment| comment.path == path && has_block(comment))
        {
            let more = place_one(comment, &owner, &self.hunks, &right, left.as_ref(), cx);
            placed.extend(more);
        }
        if let Some(sides) = self.sides.as_mut() {
            sides.comment_blocks = placed;
        }
    }
}

/// The comment's block and its empty rows; nothing when a row is outside the excerpts.
fn place_one(
    comment: &ReviewComment,
    owner: &WeakEntity<ChangesView>,
    hunks: &[HunkRows],
    right: &Pane,
    left: Option<&Pane>,
    cx: &mut App,
) -> Placed {
    let place = placement(comment, hunks, left.is_some());
    let (mine, other) = match left {
        Some(left) if place.left => (left, Some(right)),
        Some(left) => (right, Some(left)),
        None => (right, None),
    };
    let own = anchor_at(mine, place.spot, cx);
    let matching = match (other, place.other) {
        (Some(pane), Some(spot)) => anchor_at(pane, spot, cx).map(|anchor| (pane, anchor)),
        _ => None,
    };
    let Some(own) = own else {
        return Vec::new();
    };
    if other.is_some() && matching.is_none() {
        return Vec::new();
    }
    let rows = block_rows(&comment.body);
    let render = renderer(comment.clone(), owner.clone());
    let mut placed = Vec::new();
    placed.extend(insert(
        &mine.editor,
        own,
        rows,
        BlockStyle::Fixed,
        render,
        cx,
    ));
    if let Some((pane, anchor)) = matching {
        let empty: RenderBlock = Arc::new(|_| div().into_any_element());
        placed.extend(insert(
            &pane.editor,
            anchor,
            rows,
            BlockStyle::Spacer,
            empty,
            cx,
        ));
    }
    placed
}

/// A spot as a placement in the editor's own rows; None when the row is not in an excerpt.
fn anchor_at(pane: &Pane, spot: Spot, cx: &mut App) -> Option<BlockPlacement<Anchor>> {
    let last = pane.buffer.read(cx).max_point().row;
    let (row, below) = match spot {
        Spot::Above(row) if row > last => (last, true),
        Spot::Above(row) => (row, false),
        Spot::Below(row) => (row.min(last), true),
    };
    let multi = pane.editor.read(cx).buffer().read(cx);
    let anchor = multi.buffer_point_to_anchor(&pane.buffer, Point::new(row, 0), cx)?;
    Some(if below {
        BlockPlacement::Below(anchor)
    } else {
        BlockPlacement::Above(anchor)
    })
}

fn insert(
    editor: &Entity<Editor>,
    placement: BlockPlacement<Anchor>,
    rows: u32,
    style: BlockStyle,
    render: RenderBlock,
    cx: &mut App,
) -> Option<(Entity<Editor>, CustomBlockId)> {
    let id = editor.update(cx, |editor, cx| {
        editor
            .insert_blocks(
                [BlockProperties {
                    placement,
                    height: Some(rows),
                    style,
                    render,
                    priority: 0,
                }],
                None,
                cx,
            )
            .pop()
    })?;
    Some((editor.clone(), id))
}

fn renderer(comment: ReviewComment, owner: WeakEntity<ChangesView>) -> RenderBlock {
    Arc::new(move |cx: &mut BlockContext| render_block(&comment, &owner, cx))
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
    cx: &mut BlockContext,
) -> AnyElement {
    let line = cx.line_height;
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
        .w_full()
        .h(line * block_rows(&comment.body) as f32)
        .overflow_hidden()
        .bg(background)
        .border_l_2()
        .border_color(accent)
        .child(header)
        .children(rows)
        .into_any_element()
}
