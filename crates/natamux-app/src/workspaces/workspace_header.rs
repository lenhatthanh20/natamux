//! The git buttons of a workspace, under the commit box of the details panel's Changes view
//! (T-0183; before, at the end of the tab bar): Push and Create PR with their reasons, or Open
//! pull request. A done or cancelled task shows only Open pull request: its finished banner offers
//! the next git step. While files are not committed, Push waits: the commit box above commits
//! them. The buttons are labelled; the tooltip gives the reason when one is off.

use std::rc::Rc;

use gpui::ClickEvent;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::GitGraph;
use ui::{ButtonLike, Tooltip, prelude::*};

use super::workspace_rules::{GitButton, GitStep, needs_commit, pull_request_button, push_button};
use super::wrap_up_rules::is_finished_state;
use crate::app::links::open_link;
use crate::components::icons::icon_path;
use crate::components::spinner::Spinner;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct GitButtons {
    task: Task,
    graph: Option<GitGraph>,
    uncommitted: Option<u64>,
    pending: Option<GitStep>,
    on_push: OnClick,
    on_pull_request: OnClick,
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

/// A labelled git button, grey with a border (T-0168): off with its reason in the tooltip, a
/// spinner in place of its icon while its step runs.
fn git_button(
    id: &'static str,
    icon: &str,
    button: GitButton,
    busy: bool,
    other_running: bool,
    run: OnClick,
) -> AnyElement {
    let off = button.reason.is_some() || busy || other_running;
    let icon = if busy {
        Spinner::new(IconSize::Small).into_any_element()
    } else {
        Icon::from_path(icon_path(icon))
            .size(IconSize::Small)
            .color(Color::Muted)
            .into_any_element()
    };
    let tip = button.reason.clone().or(button.tip.clone());
    ButtonLike::new(id)
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Default)
        .disabled(off)
        .when_some(tip, |this, tip| this.tooltip(Tooltip::text(tip)))
        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
        .child(
            h_flex()
                .px_1p5()
                .gap_1()
                .child(icon)
                .child(Label::new(button.label).size(LabelSize::Small)),
        )
        .into_any_element()
}

fn open_pull_request(url: String) -> AnyElement {
    ButtonLike::new("workspace-open-pr")
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Default)
        .on_click(move |_, _, cx| open_link(&url, cx))
        .child(
            h_flex()
                .px_1p5()
                .gap_1()
                .child(
                    Icon::from_path(icon_path("external-link"))
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(Label::new("Open pull request").size(LabelSize::Small)),
        )
        .into_any_element()
}

impl GitButtons {
    /// Push, and Create PR or Open pull request. A finished task has only Open pull request.
    pub fn render(&self) -> Vec<AnyElement> {
        let task = &self.task;
        let finished = is_finished_state(task.state);
        let graph = self.graph.as_ref();
        let runs = |step| self.pending == Some(step);
        let other_runs = |step| self.pending.is_some() && self.pending != Some(step);
        let push_status = graph.and_then(|graph| graph.push.as_ref());
        let commit_first = !finished && needs_commit(push_status, self.uncommitted);
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
                self.on_push.clone(),
            )
            .into_any_element()
        });
        let pull_request = match &task.pr {
            Some(url) => Some(open_pull_request(url.clone())),
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
