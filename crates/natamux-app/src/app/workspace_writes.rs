//! The writes of the workspace view (screen 2), with the toasts of the React screen: Create
//! workspace and Start agent of the no-workspace view (`useTaskMutations`), Push and Create PR
//! of the header and the finished banner (`ChangesView` `gitAction`), Reopen agent of the
//! wrap-up, and closing or stopping a terminal.

use natamux_client::ClientError;
use natamux_proto::agents::start::StartAgentInput;
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::Dashboard;
use super::writes::Written;
use crate::app::source::reason;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{ToastAction, push_toast};
use crate::workspaces::workspace_rules::{GitStep, next_git_step};

impl Dashboard {
    /// A failed git step: the step's own sentence, the daemon's message under it.
    fn git_failed(error: &ClientError, text: &str, cx: &mut App) {
        push_toast(text, ToastTone::Error, Some(reason(error)), Vec::new(), cx);
    }

    /// The action of a success toast for the step that comes next (`nextGitStep`).
    fn next_step_action(&self, done: GitStep, task: &Task, cx: &Context<Self>) -> Vec<ToastAction> {
        let dashboard = cx.weak_entity();
        let id = task.id.clone();
        match next_git_step(done, task.pr.is_some()) {
            Some(GitStep::PullRequest) => vec![ToastAction::new("Create PR", move |_, cx| {
                dashboard
                    .update(cx, |dashboard, cx| {
                        dashboard.open_pull_request(&id, cx).detach();
                    })
                    .ok();
            })],
            _ => Vec::new(),
        }
    }

    /// Create workspace of a task with none (`POST /tasks/:id/workspace`).
    pub fn create_workspace(
        &mut self,
        task_id: &str,
        base: Option<String>,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.create_workspace(task_id, base.as_deref()),
            false,
            cx,
            |this, task, cx| {
                this.take_task(task);
                this.select(Some(task.id.clone()), cx);
            },
        )
    }

    /// Start agent of the no-workspace view: the daemon makes the workspace from `base` first.
    pub fn start_agent_from(
        &mut self,
        task_id: &str,
        agent: &str,
        base: Option<String>,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let input = StartAgentInput {
            agent: agent.into(),
            prompt: None,
            base,
        };
        self.write(
            source.start_agent(task_id, input),
            false,
            cx,
            |this, task, _| this.take_task(task),
        )
    }

    pub fn push(&mut self, task_id: &str, cx: &mut Context<Self>) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(source.push(task_id), true, cx, |this, task, cx| {
            this.take_task(task);
            let branch = task.branch.as_deref().unwrap_or("the branch");
            let actions = this.next_step_action(GitStep::Push, task, cx);
            push_toast(
                &format!("Pushed {branch} to origin."),
                ToastTone::Success,
                None,
                actions,
                cx,
            );
        });
        Self::on_failure(answer, "Could not push.", cx)
    }

    pub fn open_pull_request(&mut self, task_id: &str, cx: &mut Context<Self>) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(
            source.open_pull_request(task_id),
            true,
            cx,
            |this, task, cx| {
                this.take_task(task);
                let actions = task.pr.clone().map_or_else(Vec::new, |url| {
                    vec![ToastAction::new("Open", move |_, cx| cx.open_url(&url))]
                });
                push_toast(
                    "Pull request opened.",
                    ToastTone::Success,
                    None,
                    actions,
                    cx,
                );
            },
        );
        Self::on_failure(answer, "Could not open the pull request.", cx)
    }

    /// Reopen agent of the wrap-up (`POST /tasks/:id/agent/reopen`).
    pub fn reopen_agent(&mut self, task_id: &str, cx: &mut Context<Self>) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(source.reopen_agent(task_id), true, cx, |this, task, _| {
            this.take_task(task)
        });
        Self::on_failure(answer, "Could not reopen the agent.", cx)
    }

    /// Ends one terminal and its shell. `failure`: the toast when the daemon refuses, such as
    /// "Could not close Terminal 2.".
    pub fn end_terminal(
        &mut self,
        task_id: &str,
        terminal: &str,
        failure: String,
        cx: &mut Context<Self>,
    ) -> Written<()> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(
            source.close_terminal(task_id, terminal),
            true,
            cx,
            |_, _, _| {},
        );
        cx.spawn(async move |_, cx| {
            let result = answer.await;
            if let Err(error) = &result {
                cx.update(|cx| Self::git_failed(error, &failure, cx));
            }
            result
        })
    }

    /// The step's error toast once its quiet write fails; the answer goes on to the caller.
    fn on_failure<T: 'static>(
        answer: Written<T>,
        text: &'static str,
        cx: &mut Context<Self>,
    ) -> Written<T> {
        cx.spawn(async move |_, cx| {
            let result = answer.await;
            if let Err(error) = &result {
                cx.update(|cx| Self::git_failed(error, text, cx));
            }
            result
        })
    }
}
