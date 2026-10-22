//! The 44 px header of a workspace (docs/ui-spec.md 4.29, `features/workspaces/WorkspaceHeader.tsx`):
//! id, title, state chip, branch chip (click copies), then Push and Create PR with their reasons,
//! or Open pull request. A done or cancelled task shows only Open pull request: its finished
//! banner offers the next git step. Commit… (screen 4) and the chip's "Show in progress"
//! (screen 3) are not here yet; where React shows Commit…, Push says why it waits.

use std::rc::Rc;

use gpui::{ClickEvent, ClipboardItem, FontWeight};
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::GitGraph;
use ui::{ButtonLike, TintColor, Tooltip, prelude::*};

use super::workspace_rules::{GitButton, GitStep, pull_request_button, push_button};
use super::wrap_up_rules::is_finished_state;
use crate::components::chips::StateChip;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::toast;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// `COMPACT_BELOW`: under this the branch chip goes.
pub const COMPACT_BELOW: f32 = 900.;

/// Keeps the start and the end of a long branch name (`MiddleTruncate`).
pub fn middle_truncate(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max || max < 3 {
        return text.to_owned();
    }
    let keep = max - 1;
    let head: String = text.chars().take(keep.div_ceil(2)).collect();
    let tail: String = text.chars().skip(count - keep / 2).collect();
    format!("{head}\u{2026}{tail}")
}

#[derive(IntoElement)]
pub struct WorkspaceHeader {
    task: Task,
    graph: Option<GitGraph>,
    uncommitted: Option<u64>,
    pending: Option<GitStep>,
    width: Option<f32>,
    on_push: OnClick,
    on_pull_request: OnClick,
}

impl WorkspaceHeader {
    pub fn new(
        task: Task,
        graph: Option<GitGraph>,
        uncommitted: Option<u64>,
        pending: Option<GitStep>,
        width: Option<f32>,
    ) -> Self {
        WorkspaceHeader {
            task,
            graph,
            uncommitted,
            pending,
            width,
            on_push: Rc::new(|_, _| {}),
            on_pull_request: Rc::new(|_, _| {}),
        }
    }

    pub fn on_push(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_push = Rc::new(run);
        self
    }

    pub fn on_pull_request(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_pull_request = Rc::new(run);
        self
    }
}

/// A git button: off with its reason as the tooltip, busy while its step runs.
fn git_button(
    id: &'static str,
    icon: &str,
    button: GitButton,
    primary: bool,
    busy: bool,
    other_running: bool,
    run: OnClick,
) -> impl IntoElement {
    let off = button.reason.is_some() || busy || other_running;
    let tip = button.reason.clone().or(button.tip.clone());
    let style = if primary && button.reason.is_none() {
        ButtonStyle::Tinted(TintColor::Accent)
    } else {
        ButtonStyle::Outlined
    };
    let icon = if busy {
        "loader-circle".to_owned()
    } else {
        icon.to_owned()
    };
    ButtonLike::new(id)
        .style(style)
        .disabled(off)
        .when_some(tip, |button, tip| button.tooltip(Tooltip::text(tip)))
        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
        .child(
            h_flex()
                .gap_1p5()
                .px_1()
                .child(Icon::from_path(format!("icons/lucide/{icon}.svg")).size(IconSize::Small))
                .child(Label::new(button.label).size(LabelSize::Small)),
        )
}

/// The branch chip: middle-truncated, the full name in the tooltip, a click copies it.
fn branch_chip(branch: String, cx: &App) -> impl IntoElement {
    let copied = branch.clone();
    div()
        .id("workspace-branch")
        .flex_none()
        .cursor_pointer()
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .border_1()
        .border_color(cx.theme().colors().border)
        .child(
            h_flex()
                .gap_1()
                .child(
                    Icon::from_path("icons/lucide/git-branch.svg")
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(middle_truncate(&branch, 48))
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx)
                        .single_line(),
                ),
        )
        .tooltip(Tooltip::text(format!("{branch} (click to copy)")))
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
            toast("Copied the branch name.", ToastTone::Success, cx);
        })
}

fn open_pull_request(url: String, compact: bool) -> AnyElement {
    let label = if compact {
        "Open PR"
    } else {
        "Open pull request"
    };
    ButtonLike::new("workspace-open-pr")
        .style(ButtonStyle::Outlined)
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child(
            h_flex()
                .gap_1p5()
                .px_1()
                .child(Icon::from_path("icons/lucide/external-link.svg").size(IconSize::Small))
                .child(Label::new(label).size(LabelSize::Small)),
        )
        .into_any_element()
}

impl WorkspaceHeader {
    /// Push, and Create PR or Open pull request. A finished task has only Open pull request.
    fn git_buttons(&self, compact: bool) -> Vec<AnyElement> {
        let task = &self.task;
        let finished = is_finished_state(task.state);
        let graph = self.graph.as_ref();
        let runs = |step| self.pending == Some(step);
        let other_runs = |step| self.pending.is_some() && self.pending != Some(step);
        let push = (!finished).then(|| {
            let push = graph.and_then(|graph| graph.push.as_ref());
            let button = push_button(task.branch.as_deref(), push, self.uncommitted);
            let (busy, off) = (runs(GitStep::Push), other_runs(GitStep::Push));
            git_button(
                "workspace-push",
                "arrow-up-from-line",
                button,
                false,
                busy,
                off,
                self.on_push.clone(),
            )
            .into_any_element()
        });
        let pull_request = match &task.pr {
            Some(url) => Some(open_pull_request(url.clone(), compact)),
            None if !finished => {
                let button = pull_request_button(
                    task.branch.as_deref(),
                    graph.and_then(|graph| graph.ahead),
                    graph.and_then(|graph| graph.base.as_deref()),
                    self.uncommitted,
                );
                let step = GitStep::PullRequest;
                let run = self.on_pull_request.clone();
                Some(
                    git_button(
                        "workspace-create-pr",
                        "git-pull-request",
                        button,
                        true,
                        runs(step),
                        other_runs(step),
                        run,
                    )
                    .into_any_element(),
                )
            }
            None => None,
        };
        [push, pull_request].into_iter().flatten().collect()
    }
}

impl RenderOnce for WorkspaceHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let compact = self.width.is_some_and(|width| width < COMPACT_BELOW);
        let buttons = self.git_buttons(compact);
        let task = &self.task;
        let branch = task
            .branch
            .clone()
            .filter(|_| !compact)
            .map(|branch| branch_chip(branch, cx));
        h_flex()
            .h(px(44.))
            .flex_none()
            .px_4()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new(task.id.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .buffer_font(cx)
                    .single_line(),
            )
            .child(
                div().min_w_0().flex_shrink(1.).child(
                    Label::new(task.title.clone())
                        .size(LabelSize::Large)
                        .weight(FontWeight::SEMIBOLD)
                        .truncate(),
                ),
            )
            .child(StateChip::new(task.state))
            .children(branch)
            .child(div().flex_1())
            .children(buttons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_start_and_the_end_of_a_long_branch() {
        assert_eq!(middle_truncate("natamux/t-1", 20), "natamux/t-1");
        assert_eq!(middle_truncate("abcdefghij", 7), "abc\u{2026}hij");
        assert_eq!(middle_truncate("abcdefghij", 6).chars().count(), 6);
    }
}
