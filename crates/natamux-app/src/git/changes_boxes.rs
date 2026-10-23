//! The two text boxes of the Changes tab: the commit box under the files (`ChangesPanel.tsx`:
//! Generate and Commit all) and the comment box under the diff (`AgentComment.tsx`: Send to agent,
//! with the lines that Ask agent quoted).

use editor::Editor;
use editor::actions::MoveToEnd;
use gpui::{ClickEvent, Entity, Focusable, KeyContext};
use ui::{ButtonLike, Tooltip, prelude::*};

use super::ChangesView;
use crate::agents::agent_choices::{DEFAULT_AGENT, agent_label};
use crate::app::source::reason;
use crate::components::chips::Kbd;
use crate::components::form::{input_box, text_editor};
use crate::git::comment_rules::{
    REMOVE_QUOTE_LABEL, SENT_TEXT, chip_text, comment_for_agent, comment_label, placeholder,
    send_reason,
};
use crate::git::commit_rules::{commit_reason, draft_result, draft_tip};
use crate::workspaces::workspace_rules::GitStep;

impl ChangesView {
    fn message_box(&mut self, window: &mut Window, cx: &mut App) -> Entity<Editor> {
        self.message
            .get_or_insert_with(|| text_editor("Commit message", Some((3, 3)), window, cx))
            .clone()
    }

    fn comment_box(&mut self, window: &mut Window, cx: &mut App) -> Entity<Editor> {
        self.comment
            .get_or_insert_with(|| {
                text_editor("What should change, and why", Some((1, 4)), window, cx)
            })
            .clone()
    }

    /// Commit… of the header, the banner and the files-not-committed dialog: once the files are
    /// read, the box takes the focus and drafts a message when it is empty.
    pub(super) fn take_commit_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(id), Some(list)) = (self.task_id.clone(), self.list.as_ref()) else {
            return;
        };
        if self.dashboard.read(cx).commit_request.as_deref() != Some(id.as_str()) {
            return;
        }
        let has_files = !list.is_empty();
        cx.defer_in(window, move |view, window, cx| {
            if !view
                .dashboard
                .update(cx, |dashboard, _| dashboard.take_commit_request(&id))
            {
                return;
            }
            if !has_files {
                return;
            }
            let message = view.message_box(window, cx);
            window.focus(&message.focus_handle(cx), cx);
            if message.read(cx).text(cx).trim().is_empty() {
                view.draft(window, cx);
            }
        });
    }

    /// Generate (Mod+Shift+G in the box): the task's agent drafts a message. It takes the box
    /// only if the user did not type meanwhile.
    pub(super) fn draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.task_id.clone().filter(|_| !self.drafting) else {
            return;
        };
        let message = self.message_box(window, cx);
        let before = message.read(cx).text(cx);
        self.drafting = true;
        let answer = self
            .dashboard
            .update(cx, |dashboard, cx| dashboard.draft_commit_message(&id, cx));
        cx.spawn_in(window, async move |view, cx| {
            let drafted = answer.await;
            view.update_in(cx, |view, window, cx| {
                view.drafting = false;
                if let Ok(draft) = drafted {
                    let now = message.read(cx).text(cx);
                    let text = draft_result(&before, &now, &draft.message);
                    message.update(cx, |editor, cx| {
                        editor.set_text(text, window, cx);
                        editor.move_to_end(&MoveToEnd, window, cx);
                    });
                    window.focus(&message.focus_handle(cx), cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Commit all (Mod+Enter in the box): the box empties only when the commit worked.
    pub(super) fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.message_box(window, cx);
        let text = message.read(cx).text(cx).trim().to_owned();
        let committing = self.pending == Some(GitStep::Commit);
        if commit_reason(&text, self.drafting, self.git_busy(), committing).is_some() {
            return;
        }
        let files = self.list.as_ref().map_or(0, Vec::len);
        let started = self.workspace.update(cx, |workspace, cx| {
            workspace.run_git(
                GitStep::Commit,
                move |dashboard, id, cx| dashboard.commit(id, text, files, cx),
                cx,
            )
        });
        let Ok(Some(answer)) = started else {
            return;
        };
        cx.spawn_in(window, async move |_, cx| {
            if answer.await.is_ok() {
                message
                    .update_in(cx, |editor, window, cx| editor.set_text("", window, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Send to agent (Mod+Enter in the box): the comment, with the quoted lines, goes into the
    /// agent's terminal. A failure keeps the text and the quote.
    pub(super) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(id), Some(path)) = (self.task_id.clone(), self.shown_path(cx)) else {
            return;
        };
        let comment = self.comment_box(window, cx);
        let written = comment.read(cx).text(cx);
        if self.sending || send_reason(&written).is_some() {
            return;
        }
        let line = self.diff.update(cx, |diff, cx| diff.cursor_line(cx));
        let text = comment_for_agent(&path, &written, line, self.quote.as_ref());
        self.sending = true;
        self.sent = None;
        let answer = self
            .dashboard
            .update(cx, |dashboard, cx| dashboard.send_to_agent(&id, text, cx));
        cx.spawn_in(window, async move |view, cx| {
            let result = answer.await;
            view.update_in(cx, |view, window, cx| {
                view.sending = false;
                match result {
                    Ok(_) => {
                        view.quote = None;
                        view.sent = Some(Ok(()));
                        comment.update(cx, |editor, cx| editor.set_text("", window, cx));
                    }
                    Err(error) => view.sent = Some(Err(reason(&error))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Mod+Shift+A: quote the picked lines (Ask agent), or just go to the box.
    pub(super) fn comment_on_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(quote) = self.diff.update(cx, |diff, cx| diff.selection(window, cx)) {
            self.quote = Some(quote);
            self.sent = None;
        }
        let comment = self.comment_box(window, cx);
        window.focus(&comment.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn render_commit_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = self.message_box(window, cx);
        let text = message.read(cx).text(cx);
        let committing = self.pending == Some(GitStep::Commit);
        let reason = commit_reason(&text, self.drafting, self.git_busy(), committing);
        let agent = self
            .task_id
            .as_ref()
            .and_then(|id| self.dashboard.read(cx).data.task(id))
            .and_then(|task| task.agent.clone());
        let agent = agent_label(Some(agent.as_deref().unwrap_or(DEFAULT_AGENT)));
        let generate = Button::new("commit-generate", "Generate")
            .style(ButtonStyle::Subtle)
            .start_icon(Icon::new(IconName::Sparkle).size(IconSize::Small))
            .disabled(self.drafting)
            .tooltip(Tooltip::text(format!(
                "{} (Mod+Shift+G)",
                draft_tip(&agent)
            )))
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.draft(window, cx)));
        let commit = ButtonLike::new("commit-all")
            .style(ButtonStyle::Filled)
            .full_width()
            .disabled(reason.is_some() || committing)
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.commit(window, cx)))
            .child(
                h_flex()
                    .w_full()
                    .justify_center()
                    .gap_2()
                    .child(Label::new(if committing {
                        "Committing…"
                    } else {
                        "Commit all"
                    }))
                    .child(Kbd::new("mod-enter")),
            );
        let commit = match reason {
            Some(reason) => commit.tooltip(Tooltip::text(reason)).into_any_element(),
            None => commit.into_any_element(),
        };
        let mut context = KeyContext::default();
        context.add("NatamuxCommitBox");
        v_flex()
            .key_context(context)
            .flex_none()
            .p_2()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && event.keystroke.modifiers.secondary() {
                    view.commit(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(input_box(&message, cx))
            .child(
                h_flex()
                    .gap_2()
                    .child(generate)
                    .child(div().flex_1().child(commit)),
            )
            .into_any_element()
    }

    /// The chip of the lines Ask agent quoted, with its remove button.
    fn quote_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.quote.as_ref().map(|quote| {
            h_flex()
                .gap_1()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .border_1()
                .border_color(cx.theme().colors().border)
                .child(Label::new(chip_text(quote)).size(LabelSize::XSmall))
                .child(
                    IconButton::new("comment-quote-remove", IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(REMOVE_QUOTE_LABEL))
                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                            view.quote = None;
                            cx.notify();
                        })),
                )
                .into_any_element()
        })
    }

    pub(super) fn render_comment_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let comment = self.comment_box(window, cx);
        let shown_placeholder = placeholder(self.quote.as_ref());
        comment.update(cx, |editor, cx| {
            editor.set_placeholder_text(shown_placeholder, window, cx)
        });
        let written = comment.read(cx).text(cx);
        let line = self.diff.update(cx, |diff, cx| diff.cursor_line(cx));
        let label = comment_label(line, self.quote.as_ref());
        let status = self.sent.as_ref().map(|sent| match sent {
            Ok(()) => Label::new(SENT_TEXT)
                .size(LabelSize::XSmall)
                .color(Color::Success),
            Err(message) => Label::new(message.clone())
                .size(LabelSize::XSmall)
                .color(Color::Error),
        });
        let chip = self.quote_chip(cx);
        let reason = send_reason(&written);
        let send = ButtonLike::new("comment-send")
            .style(ButtonStyle::Outlined)
            .disabled(reason.is_some() || self.sending)
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.send(window, cx)))
            .child(
                h_flex()
                    .gap_2()
                    .px_1()
                    .child(Label::new("Send to agent"))
                    .child(Kbd::new("mod-enter")),
            );
        let send = match reason {
            Some(reason) => send.tooltip(Tooltip::text(reason)).into_any_element(),
            None => send.into_any_element(),
        };
        v_flex()
            .flex_none()
            .px_3()
            .py_2()
            .gap_1p5()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && event.keystroke.modifiers.secondary() {
                    view.send(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                h_flex()
                    .gap_2()
                    .child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
                    .children(status),
            )
            .children(chip)
            .child(
                h_flex()
                    .gap_2()
                    .items_end()
                    .child(div().flex_1().min_w_0().child(input_box(&comment, cx)))
                    .child(send),
            )
            .into_any_element()
    }
}
