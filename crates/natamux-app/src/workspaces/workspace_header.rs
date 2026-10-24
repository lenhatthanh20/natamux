//! The git buttons of a workspace, drawn at the end of its tab bar: Push and Create PR with
//! their reasons, or Open pull request. A done or cancelled task shows only Open pull request:
//! its finished banner offers the next git step. While origin has every commit and files are not
//! committed, Commit… takes the place of Push. In a main narrower than `ICON_ONLY_BELOW` the buttons are
//! icon-only.

use std::rc::Rc;

use gpui::ClickEvent;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::GitGraph;
use ui::{ButtonLike, Tooltip, prelude::*};

use super::workspace_rules::{GitButton, GitStep, needs_commit, pull_request_button, push_button};
use super::wrap_up_rules::is_finished_state;
use crate::components::button_style::label_color;
use crate::components::icons::icon_path;
use crate::components::spinner::Spinner;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// The main width under which the tab bar's buttons show only their icons, so the tabs keep
/// their labels beside the details dock.
pub const ICON_ONLY_BELOW: f32 = 720.;

pub struct GitButtons {
    task: Task,
    graph: Option<GitGraph>,
    uncommitted: Option<u64>,
    pending: Option<GitStep>,
    on_push: OnClick,
    on_pull_request: OnClick,
    on_commit: OnClick,
}

impl GitButtons {
    pub fn new(
        task: Task,
        graph: Option<GitGraph>,
        uncommitted: Option<u64>,
        pending: Option<GitStep>,
    ) -> Self {
        GitButtons {
            task,
            graph,
            uncommitted,
            pending,
            on_push: Rc::new(|_, _| {}),
            on_pull_request: Rc::new(|_, _| {}),
            on_commit: Rc::new(|_, _| {}),
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

    /// Commit…: opens the Changes tab, which drafts a message into an empty box.
    pub fn on_commit(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_commit = Rc::new(run);
        self
    }
}

/// A ghost git button: off with its reason as the tooltip, busy while its step runs. Icon-only
/// it always has a tooltip, with the label first.
fn git_button(
    id: &'static str,
    icon: &str,
    button: GitButton,
    busy: bool,
    other_running: bool,
    icon_only: bool,
    run: OnClick,
) -> impl IntoElement {
    let off = button.reason.is_some() || busy || other_running;
    let tip = button.reason.clone().or(button.tip.clone());
    let tip = match (icon_only, tip) {
        (true, Some(tip)) => Some(format!("{}: {tip}", button.label)),
        (true, None) => Some(button.label.to_string()),
        (false, tip) => tip,
    };
    let icon = if busy {
        Spinner::new(IconSize::Small).into_any_element()
    } else {
        Icon::from_path(icon_path(icon))
            .size(IconSize::Small)
            .into_any_element()
    };
    ButtonLike::new(id)
        .style(ButtonStyle::Subtle)
        .size(ButtonSize::Compact)
        .disabled(off)
        .when_some(tip, |button, tip| button.tooltip(Tooltip::text(tip)))
        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
        .child(
            h_flex()
                .gap_1p5()
                .px_1()
                .child(icon)
                .when(!icon_only, |row| {
                    row.child(
                        Label::new(button.label)
                            .size(LabelSize::Small)
                            .color(label_color(off)),
                    )
                }),
        )
}

fn open_pull_request(url: String, icon_only: bool) -> AnyElement {
    let label = if icon_only {
        "Open PR"
    } else {
        "Open pull request"
    };
    ButtonLike::new("workspace-open-pr")
        .style(ButtonStyle::Subtle)
        .size(ButtonSize::Compact)
        .when(icon_only, |button| {
            button.tooltip(Tooltip::text("Open pull request"))
        })
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child(
            h_flex()
                .gap_1p5()
                .px_1()
                .child(Icon::from_path(icon_path("external-link")).size(IconSize::Small))
                .when(!icon_only, |row| {
                    row.child(Label::new(label).size(LabelSize::Small))
                }),
        )
        .into_any_element()
}

impl GitButtons {
    /// Commit… or Push, and Create PR or Open pull request. A finished task has only Open pull
    /// request.
    pub fn render(&self, icon_only: bool) -> Vec<AnyElement> {
        let task = &self.task;
        let finished = is_finished_state(task.state);
        let graph = self.graph.as_ref();
        let runs = |step| self.pending == Some(step);
        let other_runs = |step| self.pending.is_some() && self.pending != Some(step);
        let push_status = graph.and_then(|graph| graph.push.as_ref());
        let commit_first = !finished && needs_commit(push_status, self.uncommitted);
        let commit = commit_first.then(|| {
            let button = GitButton {
                label: "Commit…".into(),
                reason: None,
                tip: Some("Commit your changes in the Changes tab".into()),
            };
            let off = self.pending.is_some();
            git_button(
                "workspace-commit",
                "circle-dot",
                button,
                false,
                off,
                icon_only,
                self.on_commit.clone(),
            )
            .into_any_element()
        });
        let push = (!finished && !commit_first).then(|| {
            let push = push_status;
            let button = push_button(task.branch.as_deref(), push, self.uncommitted);
            let (busy, off) = (runs(GitStep::Push), other_runs(GitStep::Push));
            git_button(
                "workspace-push",
                "arrow-up-from-line",
                button,
                busy,
                off,
                icon_only,
                self.on_push.clone(),
            )
            .into_any_element()
        });
        let pull_request = match &task.pr {
            Some(url) => Some(open_pull_request(url.clone(), icon_only)),
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
                        runs(step),
                        other_runs(step),
                        icon_only,
                        run,
                    )
                    .into_any_element(),
                )
            }
            None => None,
        };
        [commit, push, pull_request].into_iter().flatten().collect()
    }
}
