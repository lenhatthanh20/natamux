//! What the panel's clicks do: the stage card actions, the stage settings, the Details tab's
//! buttons, the "..." menu, Edit and Close. Each makes the React screen's call.

use gpui::{ClipboardItem, Entity, point};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::actions::StageAction;
use natamux_proto::stages::progress::StageProgress;
use natamux_proto::stages::resolve::resolve_steps;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::{NewTaskInput, Task};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::DetailsPanel;
use crate::agents::agent_choices::{DEFAULT_AGENT, is_agent_id};
use crate::app::dashboard::Dashboard;
use crate::app::layout_mode::DetailsMode;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;
use crate::components::scroll_rules::centred_offset;
use crate::components::toast_rules::ToastTone;
use crate::sidebar::sidebar_tree::steps_of;
use crate::stages::card_rules::end_with_the_task;
use crate::stages::send_back_dialog::{self, SendBackDialog};
use crate::stages::send_back_rules::send_back_target;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::details_rules::{MenuAction, MoreItem, asking_terminal, can_fan_out, more_menu};
use crate::tasks::edit_form::{EditForm, Saved};
use crate::tasks::fan_out_dialog;
use crate::tasks::task_confirms::{confirm_cancel_task, confirm_delete_task, confirm_keep_try};
use crate::tasks::task_rules::{actions_for, agent_action};
use crate::tasks::tries_rules::{tries_of, tries_to_cancel, try_group};
use crate::workspaces::remove_workspace_dialog;

/// `FAILED_ACTION` of `StageCard.tsx`.
pub(super) fn failure_of(action: StageAction) -> &'static str {
    match action {
        StageAction::Approve => "Could not approve the stage.",
        StageAction::Reopen => "Could not send the task back.",
        StageAction::Skip => "Could not skip the stage.",
        StageAction::Pass => "Could not pass the stage.",
        StageAction::Fail => "Could not fail the stage.",
        StageAction::Report => "Could not mark the stage done.",
        StageAction::Start => "Could not start the stage.",
    }
}

/// The task's agent when it is known, else the default one.
pub fn agent_of(task: &Task) -> String {
    task.agent
        .clone()
        .filter(|agent| is_agent_id(Some(agent)))
        .unwrap_or_else(|| DEFAULT_AGENT.to_owned())
}

impl DetailsPanel {
    /// The task's steps with their settings, and the progress of each stage. None until the
    /// settings and the events are read.
    pub(super) fn progress(&self, task: &Task) -> Option<(Vec<Step>, Vec<StageProgress>)> {
        let shown = self.shown.as_ref()?;
        let settings = shown.settings.as_ref()?.as_ref().ok()?;
        let events = accepted(shown.events.as_ref()?.as_ref().ok()?);
        let steps = resolve_steps(&steps_of(task), &settings.settings.stages);
        let mut progress = natamux_proto::stages::progress::stage_progress(task, &events, &steps);
        end_with_the_task(&mut progress, task);
        Some((steps, progress))
    }

    pub(super) fn run(
        &mut self,
        command: PanelCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.task(cx) else {
            return;
        };
        match command {
            PanelCommand::Approve(stage) => self.act(&task, StageAction::Approve, stage, cx),
            PanelCommand::Report(stage) => self.act(&task, StageAction::Report, stage, cx),
            PanelCommand::Skip(stage) => self.act(&task, StageAction::Skip, stage, cx),
            PanelCommand::Start(stage) => self.act(&task, StageAction::Start, stage, cx),
            PanelCommand::SendBack(stage) => {
                self.open_send_back(&task, &stage, window, cx);
            }
            PanelCommand::AnswerInTerminal => self.answer_in_terminal(&task, cx),
            PanelCommand::Resume => self.resume(&task, cx),
            PanelCommand::PickAgent(agent) => {
                if let Some(shown) = self.shown.as_mut() {
                    shown.agent = Some(agent);
                }
            }
            PanelCommand::StartStageAgent => {
                let agent = self
                    .shown
                    .as_ref()
                    .and_then(|shown| shown.agent.clone())
                    .unwrap_or_else(|| agent_of(&task));
                self.start_agent(&task, agent, cx);
            }
            PanelCommand::Toggle { kind, stage, on } => self.toggle(&task, kind, stage, on, cx),
            PanelCommand::FoldCard(stage) => {
                fold(self.shown.as_mut().map(|s| &mut s.open_cards), stage)
            }
            PanelCommand::FoldLane(path) => {
                fold(self.shown.as_mut().map(|s| &mut s.open_lanes), path)
            }
            PanelCommand::CreateIssue => {
                let id = task.id.clone();
                self.busy_call(cx, move |d, cx| d.create_issue(&id, cx), |_, _| {});
            }
            PanelCommand::FoldEvidence(stage) => {
                fold(self.shown.as_mut().map(|s| &mut s.open_evidence), stage)
            }
            other => self.run_details(&task, other, window, cx),
        }
        cx.notify();
    }

    fn run_details(
        &mut self,
        task: &Task,
        command: PanelCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dashboard = self.dashboard.clone();
        let id = task.id.clone();
        match command {
            PanelCommand::TaskEvent(event) => {
                self.busy_call(cx, move |d, cx| d.send_event(&id, event, cx), |_, _| {});
            }
            PanelCommand::StartAgent => self.start_agent(task, agent_of(task), cx),
            PanelCommand::CreateWorkspace => {
                let base = self.shown.as_ref().and_then(|shown| shown.base.clone());
                let opened = id.clone();
                self.busy_call(
                    cx,
                    move |d, cx| d.create_workspace(&id, base, cx),
                    move |panel, cx| panel.dashboard.update(cx, |d, cx| d.open(&opened, cx)),
                );
            }
            PanelCommand::SaveTemplate => {
                let task = task.clone();
                self.busy_call(cx, move |d, cx| d.save_template(&task, cx), |_, _| {});
            }
            PanelCommand::PickBase(base) => {
                if let Some(shown) = self.shown.as_mut() {
                    shown.base = Some(base);
                }
            }
            PanelCommand::RemoveWorkspace => {
                remove_workspace_dialog::open(&dashboard, task.clone(), window, cx)
            }
            PanelCommand::ViewChanges => {
                dashboard.update(cx, |d, cx| d.open_changes(&id, false, cx))
            }
            PanelCommand::OpenStageFile(name) => {
                dashboard.update(cx, |d, cx| d.open_stage_file(&id, &name, cx))
            }
            PanelCommand::CheckPullRequest => {
                self.busy_call(cx, move |d, cx| d.refresh_pull_request(&id, cx), |_, _| {});
            }
            PanelCommand::AddSubtask => self.add_subtask(task, window, cx),
            PanelCommand::KeepTry(kept) => self.keep_try(&kept, window, cx),
            PanelCommand::OpenTask(other) => dashboard.update(cx, |d, cx| d.choose(&other, cx)),
            PanelCommand::CopyText { text, what } => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                dashboard.update(cx, |d, cx| {
                    d.toast(&format!("Copied {what}."), ToastTone::Info, None, cx)
                });
            }
            PanelCommand::FoldSection { id, open } => {
                dashboard.update(cx, |d, cx| d.set_section(id, open, cx))
            }
            _ => {}
        }
    }

    /// The question waits in the agent's terminal of the open workspace; a sheet gives way.
    fn answer_in_terminal(&mut self, task: &Task, cx: &mut Context<Self>) {
        let list = self
            .dashboard
            .read(cx)
            .data
            .terminals
            .get(&task.id)
            .cloned();
        let terminal = asking_terminal(list.as_ref(), task);
        let id = task.id.clone();
        let sheet = self.mode != DetailsMode::Docked;
        self.dashboard.update(cx, |dashboard, cx| {
            if sheet {
                dashboard.set_details_open(false, cx);
            }
            dashboard.open_shell(&id, Some(terminal), cx);
        });
    }

    fn open_send_back(
        &mut self,
        task: &Task,
        stage: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SendBackDialog>> {
        let (_, progress) = self.progress(task)?;
        send_back_dialog::open(&self.dashboard, task, stage, &progress, window, cx)
    }

    /// `stage.send-back`: the dialog of the stage that waits, once the stages are read.
    pub fn send_back_waiting(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SendBackDialog>> {
        let task = self.task(cx)?;
        let (_, progress) = self.progress(&task)?;
        let stage = send_back_target(&progress, task.state)?.stage.clone();
        self.open_send_back(&task, &stage, window, cx)
    }

    /// The "..." menu under its button, for `--shot details-more-menu`.
    pub fn open_more_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.task(cx) else {
            return;
        };
        let items = menu_items(&task);
        let on = self.on_command(cx);
        let menu = build_more_menu(
            &items,
            &task,
            &self.dashboard,
            on,
            self.focus.clone(),
            window,
            cx,
        );
        let width = window.viewport_size().width;
        MenuLayer::open(menu, point(width - px(220.), px(76.)), window, cx);
    }

    /// Puts a part of the tab in the middle of the panel (`block: 'center'`). It needs the part
    /// drawn once, so it is called on the frame after.
    pub fn center_on(&self, index: usize, cx: &mut Context<Self>) {
        let Some(child) = self.scroll.bounds_for_item(index) else {
            return;
        };
        let view = self.scroll.bounds();
        let offset = centred_offset(
            f32::from(view.size.height),
            f32::from(child.top() - view.top()),
            f32::from(child.size.height),
            f32::from(self.scroll.max_offset().y),
        );
        self.scroll.set_offset(point(px(0.), px(offset)));
        cx.notify();
    }

    fn add_subtask(&mut self, task: &Task, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.subtask.read(cx).text(cx).trim().to_owned();
        if title.is_empty() || self.shown.as_ref().is_some_and(|shown| shown.busy) {
            return;
        }
        let input = NewTaskInput {
            title,
            parent_id: Some(task.id.clone()),
            repo: None,
        };
        let dashboard = self.dashboard.clone();
        // `create_task` selects the new task; the web keeps the selection.
        let selected = dashboard.read(cx).selected.clone();
        let written = dashboard.update(cx, |d, cx| d.create_task(input, None, cx));
        self.subtask
            .update(cx, |editor, cx| editor.set_text("", window, cx));
        cx.spawn(async move |_, cx| {
            if written.await.is_ok() {
                dashboard.update(cx, |d, cx| d.select(selected, cx));
            }
        })
        .detach();
    }

    fn keep_try(&mut self, kept: &str, window: &mut Window, cx: &mut Context<Self>) {
        let dashboard = self.dashboard.read(cx);
        let Some(task) = dashboard.data.task(kept).cloned() else {
            return;
        };
        let Some(group) = try_group(&task).map(str::to_owned) else {
            return;
        };
        let tries = tries_of(&group, &dashboard.data.tasks);
        let others: Vec<String> = tries_to_cancel(&task, &tries)
            .iter()
            .map(|other| other.id.clone())
            .collect();
        confirm_keep_try(&self.dashboard, &task, others, window, cx);
    }

    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        if self.shown.is_some() {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.set_details_open(false, cx));
        }
    }

    pub(super) fn open_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.task(cx) else {
            return;
        };
        let form = cx.new(|cx| EditForm::new(task, self.dashboard.clone(), window, cx));
        cx.subscribe(&form, |panel, _, _: &Saved, cx| {
            panel.edit = None;
            cx.notify();
        })
        .detach();
        form.update(cx, |form, cx| form.focus(window, cx));
        self.edit = Some(form);
        cx.notify();
    }

    /// The "..." button and its menu (4.13).
    pub(super) fn more_button(&self, task: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let items = menu_items(task);
        let on = self.on_command(cx);
        let task = task.clone();
        let dashboard = self.dashboard.clone();
        let focus = self.focus.clone();
        IconButton::new("details-more", IconName::Ellipsis)
            .icon_size(IconSize::Small)
            .tooltip(ui::Tooltip::text(format!("More actions for {}", task.id)))
            .disabled(items.is_empty())
            .on_click(move |event, window, cx| {
                let menu = build_more_menu(
                    &items,
                    &task,
                    &dashboard,
                    on.clone(),
                    focus.clone(),
                    window,
                    cx,
                );
                let at = event.position();
                MenuLayer::open(menu, point(at.x - px(160.), at.y + px(12.)), window, cx);
            })
    }
}

fn accepted(events: &[natamux_proto::events::task_event::StoredEvent]) -> Vec<TaskEvent> {
    events
        .iter()
        .filter(|stored| stored.accepted)
        .map(|stored| stored.event.clone())
        .collect()
}

fn fold(set: Option<&mut std::collections::BTreeSet<String>>, key: String) {
    if let Some(set) = set
        && !set.remove(&key)
    {
        set.insert(key);
    }
}

fn menu_items(task: &Task) -> Vec<MoreItem> {
    let agent = agent_action(task.state, task.agent.as_deref());
    let actions = actions_for(task.state);
    let cancel = actions.iter().find(|action| action.confirm);
    more_menu(true, can_fan_out(task.state, agent), cancel, true)
}

#[allow(clippy::too_many_arguments)]
fn build_more_menu(
    items: &[MoreItem],
    task: &Task,
    dashboard: &Entity<Dashboard>,
    on: OnCommand,
    focus: gpui::FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    let items = items.to_vec();
    let task = task.clone();
    let dashboard = dashboard.clone();
    ContextMenu::build(window, cx, move |menu, _, _| {
        items.into_iter().fold(menu.context(focus), |menu, item| {
            let MoreItem::Item {
                label,
                icon,
                danger,
                action,
            } = item
            else {
                return menu.separator();
            };
            let (task, dashboard, on) = (task.clone(), dashboard.clone(), on.clone());
            let entry = ContextMenuEntry::new(label)
                .custom_icon_path(icon_path(icon))
                .when(danger, |entry| entry.icon_color(Color::Error))
                .handler(move |window, cx| match action {
                    MenuAction::SaveTemplate => on(PanelCommand::SaveTemplate, window, cx),
                    MenuAction::RunMoreTries => fan_out_dialog::open(&dashboard, &task, window, cx),
                    MenuAction::Cancel => confirm_cancel_task(&dashboard, &task, window, cx),
                    MenuAction::Delete => confirm_delete_task(&dashboard, &task, window, cx),
                });
            menu.item(entry)
        })
    })
}
