//! The writes of the Changes tab (screen 4), with the toasts of `gitAction` in `ChangesView.tsx`:
//! Commit all, Generate, Roll back and its Undo, and Send to agent. After each success the
//! workspace's views read their git facts again, as the web's `refreshGit`.

use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::changes::{
    CommitAnswer, CommitMessageDraft, DraftSource, RollbackResult,
};
use ui::prelude::*;

use super::writes::Written;
use super::{Dashboard, DashboardChange};
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{ToastAction, push_toast};
use crate::git::commit_rules::{committed_text, rolled_back_text};
use crate::workspaces::workspace_rules::GitStep;

impl Dashboard {
    /// Commit all (`POST /tasks/:id/commit`): `files` is how many the list showed.
    pub fn commit(
        &mut self,
        task_id: &str,
        message: String,
        files: usize,
        cx: &mut Context<Self>,
    ) -> Written<CommitAnswer> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let id = task_id.to_owned();
        let answer = self.write(
            source.commit(task_id, message),
            true,
            cx,
            move |this, answer: &CommitAnswer, cx| {
                this.take_task(&answer.task);
                let actions = this.next_step_action(GitStep::Commit, &answer.task, cx);
                push_toast(
                    &committed_text(files as u64),
                    ToastTone::Success,
                    None,
                    actions,
                    cx,
                );
                cx.emit(DashboardChange::WorkspaceChanged(id));
            },
        );
        Self::on_failure(answer, "Could not commit.", cx)
    }

    /// Generate: the task's agent drafts a message. The caller puts it in the box.
    pub fn draft_commit_message(
        &mut self,
        task_id: &str,
        cx: &mut Context<Self>,
    ) -> Written<CommitMessageDraft> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(
            source.commit_message(task_id),
            true,
            cx,
            |_, draft: &CommitMessageDraft, cx| {
                if draft.source == DraftSource::Draft {
                    push_toast(
                        "The agent could not draft the message. A simple draft is in the box.",
                        ToastTone::Info,
                        draft.reason.clone(),
                        Vec::new(),
                        cx,
                    );
                }
            },
        );
        Self::on_failure(answer, "Could not draft the commit message.", cx)
    }

    /// Roll back N files: the success toast offers Undo.
    pub fn rollback(
        &mut self,
        task_id: &str,
        paths: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Written<RollbackResult> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let id = task_id.to_owned();
        let answer = self.write(
            source.rollback(task_id, paths),
            true,
            cx,
            move |_, result: &RollbackResult, cx| {
                let dashboard = cx.weak_entity();
                let undo_id = id.clone();
                let undo = ToastAction::new("Undo", move |_, cx| {
                    let id = undo_id.clone();
                    dashboard
                        .update(cx, |_, cx| cx.emit(DashboardChange::UndoRollback(id)))
                        .ok();
                });
                let text = rolled_back_text(result.files);
                push_toast(&text, ToastTone::Success, None, vec![undo], cx);
                cx.emit(DashboardChange::WorkspaceChanged(id));
            },
        );
        Self::on_failure(answer, "Could not roll back.", cx)
    }

    pub fn undo_rollback(
        &mut self,
        task_id: &str,
        cx: &mut Context<Self>,
    ) -> Written<RollbackResult> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let id = task_id.to_owned();
        let answer = self.write(
            source.undo_rollback(task_id),
            true,
            cx,
            move |_, _: &RollbackResult, cx| {
                push_toast("Rollback undone.", ToastTone::Success, None, Vec::new(), cx);
                cx.emit(DashboardChange::WorkspaceChanged(id));
            },
        );
        Self::on_failure(answer, "Could not undo the rollback.", cx)
    }

    /// Send to agent and Ask agent: the comment box shows the result, so no toast.
    pub fn send_to_agent(
        &mut self,
        task_id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.send_to_agent(task_id, text),
            true,
            cx,
            |this, task: &Task, _| this.take_task(task),
        )
    }
}
