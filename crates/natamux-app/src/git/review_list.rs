//! The Comments section at the bottom of the files panel: the open count, Show resolved, the
//! comments grouped by file, and Send N comments with its key and its error.

use gpui::ClickEvent;
use natamux_proto::workspaces::review_comments::{CommentState, ReviewComment};
use ui::{Tooltip, prelude::*};

use super::ChangesView;
use crate::components::button_style::{action_button, key_hint, quiet_button};
use crate::components::text_size::row_text;
use crate::git::review_rules::{
    NO_DRAFT_REASON, draft_count, first_line, group_by_file, open_count, quote_preview, row_label,
    send_label,
};

/// The rows scroll past this height, so the files above keep room.
const LIST_MAX_HEIGHT: f32 = 200.;

impl ChangesView {
    /// Nothing while the task has no comments.
    pub(super) fn render_comments(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.review.comments.is_empty() {
            return div().into_any_element();
        }
        let groups = group_by_file(&self.review.comments, self.review.show_resolved);
        let mut rows: Vec<AnyElement> = Vec::new();
        for group in groups {
            rows.push(
                div()
                    .px_2()
                    .pt_1()
                    .child(
                        Label::new(group.path.to_owned())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    )
                    .into_any_element(),
            );
            for comment in group.comments {
                rows.push(self.render_comment_row(comment, cx));
            }
        }
        v_flex()
            .flex_none()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(self.render_comments_header(cx))
            .child(
                v_flex()
                    .id("review-comment-list")
                    .max_h(px(LIST_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .p_1()
                    .children(rows),
            )
            .child(self.render_send(cx))
            .into_any_element()
    }

    fn render_comments_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let open = open_count(&self.review.comments);
        h_flex()
            .h(rems_from_px(32.0_f32))
            .flex_none()
            .px_2()
            .gap_1()
            .child(Label::new("Comments").size(LabelSize::Small))
            .child(
                Label::new(open.to_string())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(
                quiet_button("review-show-resolved", "Show resolved")
                    .size(ButtonSize::Compact)
                    .toggle_state(self.review.show_resolved)
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.toggle_resolved(cx))),
            )
    }

    /// One comment: id, lines and state, the first line of its body, and for an outdated one
    /// the first lines of what it was about.
    fn render_comment_row(&self, comment: &ReviewComment, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let id = comment.id.clone();
        let sent = comment.state != CommentState::Draft;
        let quote = comment.outdated.then(|| {
            v_flex()
                .child(
                    Label::new("outdated, the code changed")
                        .size(LabelSize::XSmall)
                        .color(Color::Warning),
                )
                .children(quote_preview(comment).into_iter().map(|line| {
                    Label::new(line.to_owned())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate()
                }))
        });
        v_flex()
            .id(SharedString::from(format!("review-row-{}", comment.id)))
            .px_2()
            .py_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(|this| this.bg(colors.ghost_element_hover))
            .child(
                Label::new(row_label(comment))
                    .size(LabelSize::XSmall)
                    .color(if sent { Color::Muted } else { Color::Default }),
            )
            .child(
                Label::new(first_line(&comment.body).to_owned())
                    .size(row_text())
                    .truncate(),
            )
            .children(quote)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.go_to_comment(&id, cx)))
            .into_any_element()
    }

    /// Send N comments, off with a reason while there is no draft; the error shows under it.
    fn render_send(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let drafts = draft_count(&self.review.comments);
        let sending = self.review.sending;
        let off = drafts == 0 || sending;
        let label = if sending {
            "Sending\u{2026}".to_owned()
        } else {
            send_label(drafts)
        };
        let send = action_button("review-send", label)
            .full_width()
            .key_binding(key_hint("mod-alt-enter", off))
            .disabled(off)
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.send_comments(cx)));
        let send = if drafts == 0 {
            send.tooltip(Tooltip::text(NO_DRAFT_REASON))
        } else {
            send
        };
        let error = self.review.error.clone().map(|error| {
            Label::new(error)
                .size(LabelSize::XSmall)
                .color(Color::Error)
        });
        v_flex()
            .flex_none()
            .p_2()
            .gap_1()
            .child(send)
            .children(error)
    }
}
