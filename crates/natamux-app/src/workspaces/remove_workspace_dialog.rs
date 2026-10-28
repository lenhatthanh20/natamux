//! The Remove workspace dialog (`RemoveWorkspaceDialog.tsx`, docs/ui-spec.md 4.29). It says what
//! goes and what stays, lists the files that are not committed first (Remove anyway discards
//! them), and stays open, busy, until the daemon answers.

use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, svg};
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph};
use ui::{ButtonLike, Checkbox, prelude::*};

use super::finished_rules::{FinishedTask, branch_choice};
use super::remove_dialog_rules::{
    KEEPS, NOT_COMMITTED, REMOVE_ANYWAY, agent_stops, busy_text, confirm_label, discards_text,
    not_committed_title, remove_title, shells_gone, shown_files,
};
use crate::app::dashboard::Dashboard;
use crate::app::source::reason;
use crate::components::button_style::action_button;
use crate::components::colors::change_color;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::icons::icon_path;
use crate::git::change_rules::status_tone;

/// Opens the dialog for the task's workspace, Cancel focused.
pub fn open(dashboard: &Entity<Dashboard>, task: Task, window: &mut Window, cx: &mut App) {
    open_with(dashboard, task, false, window, cx);
}

/// `discard`: as after Remove anyway, for the shots.
pub fn open_with(
    dashboard: &Entity<Dashboard>,
    task: Task,
    discard: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    let view = cx.new(|cx| RemoveWorkspaceDialog::new(dashboard, task, discard, cx));
    let cancel = view.read(cx).cancel.clone();
    DialogLayer::open(view, window, cx);
    window.focus(&cancel, cx);
}

pub struct RemoveWorkspaceDialog {
    dashboard: Entity<Dashboard>,
    task: Task,
    /// None while read. A failed read counts as no files, as the web's `changes.data ?? []`.
    changes: Option<Vec<ChangeEntry>>,
    graph: Option<GitGraph>,
    shells: Option<TerminalList>,
    keep_branch: bool,
    /// Remove anyway was picked: the files that are not committed go with the folder.
    discard: bool,
    removing: bool,
    error: Option<String>,
    focus: FocusHandle,
    cancel: FocusHandle,
    view_changes: FocusHandle,
    anyway: FocusHandle,
    branch: FocusHandle,
    submit: FocusHandle,
    _reads: Vec<gpui::Task<()>>,
}

impl RemoveWorkspaceDialog {
    fn new(
        dashboard: Entity<Dashboard>,
        task: Task,
        discard: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut dialog = RemoveWorkspaceDialog {
            dashboard,
            task,
            changes: None,
            graph: None,
            shells: None,
            keep_branch: false,
            discard,
            removing: false,
            error: None,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
            view_changes: cx.focus_handle(),
            anyway: cx.focus_handle(),
            branch: cx.focus_handle(),
            submit: cx.focus_handle(),
            _reads: Vec::new(),
        };
        dialog.read_git(cx);
        dialog
    }

    fn read_git(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            self.changes = Some(Vec::new());
            return;
        };
        let id = self.task.id.clone();
        let changes = source.list_changes(&id);
        let graph = source.graph(&id);
        let shells = source.list_terminals(&id);
        self._reads = vec![
            cx.spawn(async move |this, cx| {
                let changes = changes.await.unwrap_or_default();
                this.update(cx, |this, cx| {
                    this.changes = Some(changes);
                    cx.notify();
                })
                .ok();
            }),
            cx.spawn(async move |this, cx| {
                // The box stays at "Only when it is merged." without a graph.
                if let Ok(graph) = graph.await {
                    this.update(cx, |this, cx| {
                        this.graph = Some(graph);
                        cx.notify();
                    })
                    .ok();
                }
            }),
            cx.spawn(async move |this, cx| {
                if let Ok(shells) = shells.await {
                    this.update(cx, |this, cx| {
                        this.shells = Some(shells);
                        cx.notify();
                    })
                    .ok();
                }
            }),
        ];
    }

    fn not_committed(&self) -> Option<&[ChangeEntry]> {
        self.changes.as_deref().filter(|files| !files.is_empty())
    }

    /// The list of files that are not committed, until Remove anyway is picked.
    fn asks_to_commit(&self) -> bool {
        !self.discard && self.not_committed().is_some()
    }

    fn remove_anyway(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.discard = true;
        window.focus(&self.cancel, cx);
        cx.notify();
    }

    fn delete_branch(&self) -> bool {
        let choice = branch_choice(&FinishedTask::from_task(&self.task), self.graph.as_ref());
        choice.can_delete && !self.keep_branch
    }

    /// View changes and Commit…: closes the dialog, then shows the Changes tab of the task, which
    /// drafts a message into an empty commit box for Commit….
    fn open_changes(&mut self, commit: bool, cx: &mut Context<Self>) {
        let id = self.task.id.clone();
        cx.emit(DismissEvent);
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.open_changes(&id, commit, cx));
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.removing || self.changes.is_none() || self.asks_to_commit() {
            return;
        }
        self.removing = true;
        self.error = None;
        let delete_branch = self.delete_branch();
        // Only files the person saw listed are discarded; with none, the daemon still checks.
        let force = self.discard && self.not_committed().is_some();
        let id = self.task.id.clone();
        let written = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.remove_workspace(&id, delete_branch, force, cx)
        });
        cx.spawn(async move |this, cx| {
            let result = written.await;
            this.update(cx, |this, cx| match result {
                Ok(_) => cx.emit(DismissEvent),
                Err(error) => {
                    this.removing = false;
                    this.error = Some(reason(&error));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

impl EventEmitter<DismissEvent> for RemoveWorkspaceDialog {}

impl Focusable for RemoveWorkspaceDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for RemoveWorkspaceDialog {
    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        if self.asks_to_commit() {
            return vec![
                self.cancel.clone(),
                self.view_changes.clone(),
                self.anyway.clone(),
                self.submit.clone(),
            ];
        }
        let mut ring = vec![self.cancel.clone()];
        if self.task.branch.is_some() {
            ring.insert(0, self.branch.clone());
        }
        ring.push(self.submit.clone());
        ring
    }

    fn busy(&self, _: &App) -> bool {
        self.removing
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.asks_to_commit() {
            self.open_changes(true, cx);
            return;
        }
        self.confirm(cx);
    }
}

/// One line of what goes or stays, with its mark.
fn row(icon: &'static str, color: Color, text: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_2p5()
        .child(
            svg()
                .path(icon_path(icon))
                .size_3p5()
                .mt_0p5()
                .flex_none()
                .text_color(color.color(cx)),
        )
        .child(div().min_w_0().child(text))
}

impl RemoveWorkspaceDialog {
    fn render_changes(&self, files: &[ChangeEntry], cx: &mut Context<Self>) -> AnyElement {
        let (shown, more) = shown_files(files);
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let list = v_flex()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().colors().editor_background)
            .children(shown.iter().map(|file| {
                h_flex()
                    .gap_2()
                    .child(
                        Label::new(file.status.letter())
                            .size(LabelSize::Small)
                            .color(Color::Custom(change_color(status_tone(file.status), cx)))
                            .buffer_font(cx),
                    )
                    .child(
                        Label::new(file.path.clone())
                            .size(LabelSize::Small)
                            .buffer_font(cx)
                            .truncate(),
                    )
            }));
        DialogFrame::new(not_committed_title(files.len()), close)
            .description(NOT_COMMITTED)
            .child(list)
            .children(more.map(|more| Label::new(more).size(LabelSize::Small).color(Color::Muted)))
            .footer(cancel_button("Close", &self.cancel, cancel))
            .footer(
                action_button("remove-view-changes", "View changes")
                    .track_focus(&self.view_changes)
                    .on_click(cx.listener(|this, _, _, cx| this.open_changes(false, cx))),
            )
            .footer(
                action_button("remove-anyway", REMOVE_ANYWAY)
                    .track_focus(&self.anyway)
                    .on_click(cx.listener(|this, _, window, cx| this.remove_anyway(window, cx))),
            )
            .footer(primary_button(
                "Commit\u{2026}",
                &self.submit,
                false,
                cx.listener(|this, _, _, cx| this.open_changes(true, cx)),
            ))
            .into_any_element()
    }

    fn render_branch(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.task.branch.as_ref()?;
        let choice = branch_choice(&FinishedTask::from_task(&self.task), self.graph.as_ref());
        let checked = self.delete_branch();
        let disabled = !choice.can_delete || self.removing;
        let text_color = if choice.can_delete {
            Color::Default
        } else {
            Color::Muted
        };
        let toggle = ButtonLike::new("remove-branch")
            .track_focus(&self.branch)
            .disabled(disabled)
            .on_click(cx.listener(|this, _, _, cx| {
                this.keep_branch = !this.keep_branch;
                cx.notify();
            }))
            .child(
                h_flex()
                    .items_start()
                    .gap_2p5()
                    .child(
                        Checkbox::new("remove-branch-box", checked.into())
                            .disabled(disabled)
                            .visualization_only(true),
                    )
                    .child(
                        v_flex()
                            .child(Label::new("Also delete the branch").color(text_color))
                            .child(
                                Label::new(choice.note)
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                    ),
            );
        Some(toggle.into_any_element())
    }

    fn render_remove(&self, cx: &mut Context<Self>) -> AnyElement {
        let task = &self.task;
        let gone = shells_gone(self.shells.as_ref());
        let discarded = self
            .not_committed()
            .filter(|_| self.discard)
            .map(<[_]>::len);
        let label = confirm_label(!gone.busy.is_empty(), self.removing, discarded.is_some());
        let folder = v_flex().child(Label::new("Deletes the folder")).child(
            Label::new(task.workspace.clone().unwrap_or_default())
                .size(LabelSize::Small)
                .color(Color::Muted),
        );
        let mut rows = Vec::new();
        if let Some(count) = discarded {
            let text = Label::new(discards_text(count));
            rows.push(row("triangle-alert", Color::Error, text, cx).into_any_element());
        }
        rows.push(row("minus", Color::Error, folder, cx).into_any_element());
        if let Some(text) = agent_stops(task) {
            rows.push(
                row("triangle-alert", Color::Warning, Label::new(text), cx).into_any_element(),
            );
        }
        for (terminal, program) in &gone.busy {
            let text = Label::new(busy_text(terminal, program));
            rows.push(row("triangle-alert", Color::Warning, text, cx).into_any_element());
        }
        if let Some(text) = gone.plain {
            rows.push(row("minus", Color::Error, Label::new(text), cx).into_any_element());
        }
        rows.extend(self.render_branch(cx));
        rows.push(row("check", Color::Success, Label::new(KEEPS), cx).into_any_element());

        let close = cx.listener(|this, _, _, cx| {
            if !this.removing {
                cx.emit(DismissEvent);
            }
        });
        let cancel = cx.listener(|this, _, _, cx| {
            if !this.removing {
                cx.emit(DismissEvent);
            }
        });
        let submit = cx.listener(|this, _, _, cx| this.confirm(cx));
        // Reading the changes: the button waits, as the web's "Reading the changes…".
        let waiting = self.changes.is_none() || self.removing;
        DialogFrame::new(remove_title(&task.id), close)
            .child(v_flex().gap_3().children(rows))
            .children(self.error.clone().map(|error| {
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .bg(cx.theme().colors().editor_background)
                    .child(Label::new(error).size(LabelSize::Small).color(Color::Error))
            }))
            .footer(cancel_button("Cancel", &self.cancel, cancel).disabled(self.removing))
            .footer(primary_button(label, &self.submit, waiting, submit))
            .into_any_element()
    }
}

impl Render for RemoveWorkspaceDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.not_committed().filter(|_| !self.discard) {
            Some(files) => {
                let files = files.to_vec();
                self.render_changes(&files, cx)
            }
            None => self.render_remove(cx),
        };
        div().track_focus(&self.focus).child(body)
    }
}
