//! The writes of the details panel (screen 3), with the toasts of the React screen
//! (`useTaskMutations`, `StageCard.tsx`): stage actions and stage settings, Edit, Save as
//! template, Run more tries, Keep this try, Create issue and Check now.

use natamux_proto::stages::actions::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskChanges};
use natamux_proto::tasks::template::{TaskTemplate, TemplateInput};
use natamux_proto::workspaces::workspace::FanOutInput;
use ui::prelude::*;

use super::Dashboard;
use super::writes::Written;
use crate::app::source::reason;
use crate::components::toast_rules::ToastTone;
use crate::tasks::details_commands::StageToggle;

/// `templateMutation`: a task title can be longer than a template allows.
const TEMPLATE_NAME_MAX: usize = 100;
const TEMPLATE_TITLE_MAX: usize = 200;

fn first_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

impl Dashboard {
    /// A failed write of the panel: its own sentence, the daemon's message under it.
    fn panel_failed<T: 'static>(
        &mut self,
        answer: Written<T>,
        text: &'static str,
        cx: &mut Context<Self>,
    ) -> Written<T> {
        cx.spawn(async move |this, cx| {
            let result = answer.await;
            if let Err(error) = &result {
                let detail = reason(error);
                this.update(cx, |this, cx| {
                    this.toast(text, ToastTone::Error, Some(detail), cx)
                })
                .ok();
            }
            result
        })
    }

    /// A button of a stage card (`POST /tasks/:id/stage`). `failure`: its `FAILED_ACTION` text.
    pub fn stage_action(
        &mut self,
        task_id: &str,
        input: StageActionInput,
        failure: &'static str,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(
            source.stage_action(task_id, input),
            true,
            cx,
            |this, task, _| this.take_task(task),
        );
        self.panel_failed(answer, failure, cx)
    }

    /// The workflow, CI and skip boxes of a stage (`PUT /tasks/:id/workflow|ci|skip`).
    pub fn set_stage_toggle(
        &mut self,
        task_id: &str,
        kind: StageToggle,
        stage: &str,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let call = source.set_stage_toggle(task_id, kind, stage, on);
        let answer = self.write(call, true, cx, |this, task, _| this.take_task(task));
        self.panel_failed(answer, kind.failure(), cx)
    }

    /// Save of the edit form (`PATCH /tasks/:id`).
    pub fn update_task(
        &mut self,
        task_id: &str,
        changes: TaskChanges,
        cx: &mut Context<Self>,
    ) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.update_task(task_id, changes),
            false,
            cx,
            |this, task, _| this.take_task(task),
        )
    }

    pub fn save_template(&mut self, task: &Task, cx: &mut Context<Self>) -> Written<TaskTemplate> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let input = TemplateInput {
            name: first_chars(&task.title, TEMPLATE_NAME_MAX),
            title: first_chars(&task.title, TEMPLATE_TITLE_MAX),
            description: task.description.clone(),
            priority: task.priority.clone(),
            labels: task.labels.clone(),
        };
        self.write(
            source.save_template(input),
            false,
            cx,
            |this, template, cx| {
                let text = format!("Saved as the template \"{}\".", template.name);
                this.toast(&text, ToastTone::Info, None, cx);
            },
        )
    }

    /// Run more tries (`POST /tasks/:id/fan-out`): the list then shows the tries side by side.
    pub fn fan_out(
        &mut self,
        task_id: &str,
        count: u32,
        start: bool,
        cx: &mut Context<Self>,
    ) -> Written<Vec<Task>> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let search = format!("tries-{}", task_id.to_lowercase());
        let answer = source.fan_out(task_id, FanOutInput { count, start });
        self.write(answer, false, cx, move |this, created, cx| {
            for task in created {
                this.take_task(task);
            }
            this.set_filter(search, cx);
            let text = if created.len() == 1 {
                "Made 1 try.".to_owned()
            } else {
                format!("Made {} tries.", created.len())
            };
            this.toast(&text, ToastTone::Info, None, cx);
        })
    }

    /// Keep this try: the other tries that are not finished are cancelled, one after another.
    pub fn keep_try(&mut self, others: Vec<String>, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let mut cancelled = Vec::new();
            for id in others {
                let input =
                    natamux_proto::events::task_event::TaskEventInput::human("task.cancelled");
                match source.send_event(&id, input).await {
                    Ok(answer) => cancelled.push(answer.task),
                    Err(error) => {
                        let text = reason(&error);
                        this.update(cx, |this, cx| this.toast(&text, ToastTone::Error, None, cx))
                            .ok();
                        return;
                    }
                }
            }
            this.update(cx, |this, cx| {
                for task in &cancelled {
                    this.take_task(task);
                }
                let text = if cancelled.len() == 1 {
                    "Cancelled 1 other try.".to_owned()
                } else {
                    format!("Cancelled {} other tries.", cancelled.len())
                };
                this.toast(&text, ToastTone::Info, None, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Create issue of the Progress tab (`POST /tasks/:id/issue`).
    pub fn create_issue(&mut self, task_id: &str, cx: &mut Context<Self>) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        let answer = self.write(source.create_issue(task_id), true, cx, |this, task, cx| {
            this.take_task(task);
            if let Some(issue) = &task.issue {
                let text = format!("Opened issue {}.", issue.key);
                this.toast(&text, ToastTone::Success, None, cx);
            }
        });
        self.panel_failed(answer, "Could not open the issue.", cx)
    }

    /// Check now of the Pull request section: git is read now, not the answer of a timed read.
    pub fn refresh_pull_request(&mut self, task_id: &str, cx: &mut Context<Self>) -> Written<Task> {
        let Some(source) = self.source.clone() else {
            return Self::no_daemon();
        };
        self.write(
            source.refresh_pull_request(task_id),
            false,
            cx,
            |this, task, _| this.take_task(task),
        )
    }
}
