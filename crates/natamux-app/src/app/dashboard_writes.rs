//! The dashboard's writes: each makes the daemon call of the React screen, merges the task the
//! daemon answers with, and toasts the result (`useTaskMutations`). A write the dialog waits for
//! gives its error back instead of a toast, so the dialog shows it.

use natamux_client::{Answer, ClientError, SendEventAnswer};
use natamux_proto::agents::start::StartAgentInput;
use natamux_proto::events::task_event::TaskEventInput;
use natamux_proto::tasks::task::{DeleteTaskAnswer, NewTaskInput, Task, TemplateTaskInput};
use natamux_proto::workspaces::workspace::{NewWorkspaceInput, RemoveWorkspaceAnswer};
use ui::prelude::*;

use super::Dashboard;
use crate::app::dashboard_data::BulkRun;
use crate::app::source::{delete_all, reason, remove_all};
use crate::components::toast_rules::ToastTone;
use crate::tasks::delete_finished::{DeleteOutcome, delete_summary};
use crate::tasks::task_rules::actions_for;
use crate::workspaces::finished_rules::removed_text;
use crate::workspaces::remove_finished_rules::{SummaryTone, remove_summary};

/// What a write gives back to the view that asked for it.
pub type Written<T> = gpui::Task<Result<T, ClientError>>;

impl Dashboard {
    /// Runs one daemon call. `done` takes the answer; an error is toasted unless `quiet`.
    fn write<T: 'static>(
        &mut self,
        answer: Answer<T>,
        quiet: bool,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, &T, &mut Context<Self>) + 'static,
    ) -> Written<T> {
        cx.spawn(async move |this, cx| {
            let result = answer.await;
            this.update(cx, |this, cx| {
                match &result {
                    Ok(value) => done(this, value, cx),
                    Err(error) if !quiet => this.toast(&reason(error), ToastTone::Error, None, cx),
                    Err(_) => {}
                }
                cx.notify();
            })
            .ok();
            result
        })
    }

    fn no_daemon<T: 'static>() -> Written<T> {
        gpui::Task::ready(Err(ClientError::BadAnswer("the app has no daemon".into())))
    }

    fn take_task(&mut self, task: &Task) {
        self.data.merge_task(task.clone());
    }

    /// Quick add (`n`) and Add subtask: a task, or one from a template when `template` is set.
    pub fn create_task(
        &mut self,
        input: NewTaskInput,
        template: Option<String>,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = match template {
            Some(template) if input.parent_id.is_none() => source.create_from_template(
                &template,
                TemplateTaskInput {
                    title: input.title,
                    repo: input.repo,
                },
            ),
            _ => source.create_task(input),
        };
        self.write(answer, false, cx, |this, task, cx| {
            this.take_task(task);
            this.select(Some(task.id.clone()), cx);
        })
    }

    /// The Start a task dialog. The workspace is made after the answer; a toast says when.
    pub fn new_workspace(
        &mut self,
        input: NewWorkspaceInput,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let base = input.repo.clone().zip(input.base.clone());
        self.write(
            source.new_workspace(input),
            true,
            cx,
            move |this, task, cx| {
                this.take_task(task);
                this.select(Some(task.id.clone()), cx);
                if let Some((repo, base)) = &base {
                    this.view.remember_base(repo, base);
                    this.save_view(cx);
                }
                this.toast(
                    &format!("{} is being prepared.", task.id),
                    ToastTone::Info,
                    None,
                    cx,
                );
                if let Some(notice) = this.data.announce(&task.id) {
                    this.toast(&notice.text, notice.tone, None, cx);
                }
            },
        )
    }

    /// A state action of a row menu or a key, such as `review.approved`.
    pub fn send_event(
        &mut self,
        task_id: &str,
        kind: &str,
        cx: &mut Context<Self>,
    ) -> Written<SendEventAnswer> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = source.send_event(task_id, TaskEventInput::human(kind));
        self.write(answer, false, cx, |this, answer, cx| {
            this.take_task(&answer.task);
            if !answer.accepted {
                let text = format!(
                    "The daemon did not accept that change. {} stays {}.",
                    answer.task.id,
                    answer.task.state.name()
                );
                this.toast(&text, ToastTone::Error, None, cx);
            }
        })
    }

    /// A state action key (a, r and the others) on the selected task, when its state offers it
    /// without asking. It works from the list, the tree and an open task.
    pub fn task_action(&mut self, event: &'static str, cx: &mut Context<Self>) {
        let Some(task) = self.selected.as_deref().and_then(|id| self.data.task(id)) else {
            return;
        };
        let offered = actions_for(task.state)
            .iter()
            .any(|action| action.event_type == event && !action.confirm);
        if offered {
            let id = task.id.clone();
            self.send_event(&id, event, cx).detach();
        }
    }

    pub fn rename_task(
        &mut self,
        task_id: &str,
        title: &str,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.rename_task(task_id, title),
            false,
            cx,
            |this, task, _| this.take_task(task),
        )
    }

    pub fn delete_task(
        &mut self,
        task_id: &str,
        cx: &mut Context<Self>,
    ) -> Written<DeleteTaskAnswer> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.delete_task(task_id),
            false,
            cx,
            |this, answer, cx| {
                this.forget(&answer.id, cx);
                for task in &answer.tasks {
                    this.take_task(task);
                }
                this.toast(
                    &format!("Deleted {}.", answer.id),
                    ToastTone::Info,
                    None,
                    cx,
                );
            },
        )
    }

    /// `forgetTask`: a deleted task leaves the selection, main and the pinned list.
    fn forget(&mut self, id: &str, cx: &mut Context<Self>) {
        self.data.forget_task(id);
        if self.selected.as_deref() == Some(id) {
            self.selected = None;
        }
        if self.open_task.as_deref() == Some(id) {
            self.back(cx);
        }
        if self.view.pinned.iter().any(|pinned| pinned == id) {
            self.toggle_pin(id, cx);
        }
    }

    /// The Remove workspace dialog waits for this and shows its error.
    pub fn remove_workspace(
        &mut self,
        task_id: &str,
        delete_branch: bool,
        cx: &mut Context<Self>,
    ) -> Written<RemoveWorkspaceAnswer> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let branch = self.data.task(task_id).and_then(|task| task.branch.clone());
        let answer = source.remove_workspace(task_id, delete_branch);
        self.write(answer, true, cx, move |this, answer, cx| {
            this.take_task(&answer.task);
            let text = removed_text(&answer.task.id, branch.as_deref(), answer.branch_deleted);
            this.toast(&text, ToastTone::Info, None, cx);
        })
    }

    pub fn remove_repo(&mut self, repo_id: &str, cx: &mut Context<Self>) -> Written<()> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let name = self
            .data
            .repos
            .iter()
            .find(|repo| repo.id == repo_id)
            .map_or_else(|| repo_id.to_owned(), |repo| repo.name.clone());
        let repo_id = repo_id.to_owned();
        self.write(
            source.remove_repo(&repo_id),
            false,
            cx,
            move |this, _, cx| {
                this.data.repos.retain(|repo| repo.id != repo_id);
                this.toast(&format!("Removed {name}."), ToastTone::Info, None, cx);
            },
        )
    }

    pub fn start_agent(
        &mut self,
        task_id: &str,
        agent: &str,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let input = StartAgentInput {
            agent: agent.into(),
            ..StartAgentInput::default()
        };
        self.write(
            source.start_agent(task_id, input),
            false,
            cx,
            |this, task, _| this.take_task(task),
        )
    }

    // The wrap-up of screen 2 ends terminals.
    #[allow(dead_code)]
    pub fn close_terminal(
        &mut self,
        task_id: &str,
        terminal: &str,
        cx: &mut Context<Self>,
    ) -> Written<()> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.close_terminal(task_id, terminal),
            false,
            cx,
            |_, _, _| {},
        )
    }

    /// Delete all done or cancelled: one task after the other, then one summary toast.
    pub fn delete_tasks(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if !self.bulk.start(BulkRun::DeleteTasks) {
            return;
        }
        cx.notify();
        let outcomes = cx
            .background_executor()
            .spawn(async move { delete_all(&source, &ids) });
        cx.spawn(async move |this, cx| {
            let (outcomes, related) = outcomes.await;
            this.update(cx, |this, cx| {
                this.bulk.finish(BulkRun::DeleteTasks);
                for outcome in &outcomes {
                    if let DeleteOutcome::Deleted { id } = outcome {
                        this.forget(id, cx);
                    }
                }
                for task in &related {
                    this.take_task(task);
                }
                let summary = delete_summary(&outcomes);
                this.toast(&summary.text, summary.tone, summary.detail, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Remove finished workspaces: one after the other, then one summary toast.
    pub fn remove_workspaces(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if !self.bulk.start(BulkRun::RemoveFinished) {
            return;
        }
        cx.notify();
        let outcomes = cx
            .background_executor()
            .spawn(async move { remove_all(&source, &ids) });
        cx.spawn(async move |this, cx| {
            let (outcomes, changed) = outcomes.await;
            this.update(cx, |this, cx| {
                this.bulk.finish(BulkRun::RemoveFinished);
                for task in &changed {
                    this.take_task(task);
                }
                let summary = remove_summary(&outcomes);
                let tone = match summary.tone {
                    SummaryTone::Success => ToastTone::Success,
                    SummaryTone::Error => ToastTone::Error,
                };
                this.toast(&summary.text, tone, summary.detail, cx);
            })
            .ok();
        })
        .detach();
    }
}
