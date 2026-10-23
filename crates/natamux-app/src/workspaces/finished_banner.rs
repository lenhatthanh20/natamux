//! The 40 px banner under the header of a done or cancelled task (docs/ui-spec.md 4.29,
//! `features/workspaces/FinishedBanner.tsx`): "Task done." and the next git step, the first that
//! matches (`finishedBanner`). Remove workspace is always there; it waits while files are not
//! committed.

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{TintColor, Tooltip, prelude::*};

use super::workspace_rules::{GitStep, push_button};
use super::wrap_up_rules::{FinishedAction, FinishedBannerText, GitFacts, finished_banner};
use crate::components::colors::state_colors;

pub type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// What the banner's buttons do.
#[derive(Clone)]
pub struct FinishedActions {
    pub push: OnClick,
    pub pull_request: OnClick,
    /// Commit…: the Changes tab, which drafts a message into an empty box.
    pub commit: OnClick,
    pub view_changes: OnClick,
    pub remove: OnClick,
}

#[derive(IntoElement)]
pub struct FinishedBanner {
    task: Task,
    facts: GitFacts,
    pending: Option<GitStep>,
    actions: FinishedActions,
}

impl FinishedBanner {
    pub fn new(
        task: Task,
        facts: GitFacts,
        pending: Option<GitStep>,
        actions: FinishedActions,
    ) -> Self {
        FinishedBanner {
            task,
            facts,
            pending,
            actions,
        }
    }
}

fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    primary: bool,
    disabled: bool,
    run: OnClick,
) -> Button {
    Button::new(id, label)
        .label_size(LabelSize::Small)
        .style(if primary {
            ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ButtonStyle::Outlined
        })
        .disabled(disabled)
        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
}

impl FinishedBanner {
    /// The next git step's button, the first that matches.
    fn main_button(&self, primary: Option<FinishedAction>) -> Option<AnyElement> {
        let busy = self.pending.is_some();
        let graph = self.facts.graph.as_ref();
        match primary? {
            FinishedAction::Push => {
                let push = push_button(
                    self.task.branch.as_deref(),
                    graph.and_then(|graph| graph.push.as_ref()),
                    self.facts.uncommitted,
                );
                let off = busy || push.reason.is_some();
                let tip = push.reason.or(push.tip);
                let main = button(
                    "finished-push",
                    push.label,
                    true,
                    off,
                    self.actions.push.clone(),
                );
                Some(match tip {
                    Some(tip) => main.tooltip(Tooltip::text(tip)).into_any_element(),
                    None => main.into_any_element(),
                })
            }
            FinishedAction::CreatePullRequest => {
                let run = self.actions.pull_request.clone();
                Some(button("finished-create-pr", "Create PR", true, busy, run).into_any_element())
            }
            FinishedAction::OpenPullRequest => self.task.pr.clone().map(|url| {
                let open: OnClick = Rc::new(move |_: &mut Window, cx: &mut App| cx.open_url(&url));
                button("finished-open-pr", "Open pull request", true, false, open)
                    .into_any_element()
            }),
            FinishedAction::Remove => {
                let run = self.actions.remove.clone();
                Some(
                    button("finished-remove", "Remove workspace", true, busy, run)
                        .into_any_element(),
                )
            }
            FinishedAction::Commit => {
                let run = self.actions.commit.clone();
                Some(button("finished-commit", "Commit…", true, busy, run).into_any_element())
            }
            FinishedAction::ViewChanges => {
                let run = self.actions.view_changes.clone();
                Some(
                    button("finished-view-changes", "View changes", true, false, run)
                        .into_any_element(),
                )
            }
        }
    }

    /// Remove workspace beside the next step, off with its reason while files are not committed.
    fn secondary_remove(&self, text: &FinishedBannerText) -> Option<Button> {
        if text.primary == Some(FinishedAction::Remove) {
            return None;
        }
        let off = text.remove_reason.is_some() || self.pending.is_some();
        let remove = button(
            "finished-remove",
            "Remove workspace",
            false,
            off,
            self.actions.remove.clone(),
        );
        Some(match &text.remove_reason {
            Some(reason) => remove.tooltip(Tooltip::text(reason.clone())),
            None => remove,
        })
    }
}

impl RenderOnce for FinishedBanner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = finished_banner(&self.task, &self.facts);
        let state = self.task.state;
        let tone = state_colors(state, cx);
        let icon = if state == TaskState::Done {
            "circle-check"
        } else {
            "ban"
        };
        let main = self.main_button(text.primary);
        let secondary = self.secondary_remove(&text);
        let reason = text
            .remove_reason
            .clone()
            .filter(|_| text.primary != Some(FinishedAction::Remove))
            .map(|reason| {
                Label::new(reason)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
            });
        let sentence = text.text.map(|sentence| {
            div()
                .min_w_0()
                .flex_shrink(1.)
                .child(Label::new(sentence).size(LabelSize::Small).truncate())
        });
        h_flex()
            .h(px(40.))
            .flex_none()
            .px_4()
            .gap_2()
            .bg(tone.bg)
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Icon::from_path(format!("icons/lucide/{icon}.svg"))
                    .size(IconSize::Medium)
                    .color(Color::Custom(tone.fg)),
            )
            .child(
                Label::new(text.title)
                    .size(LabelSize::Small)
                    .weight(FontWeight::SEMIBOLD),
            )
            .children(sentence)
            .child(div().flex_1())
            .children(reason)
            .children(secondary)
            .children(main)
    }
}
