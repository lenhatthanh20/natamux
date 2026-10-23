//! The Roll back dialog (`RollbackDialog.tsx`, docs/ui-spec.md 4.14): the files the task changed,
//! checked, and a danger button. Confirm closes the dialog; the workspace then runs the git step,
//! so its toast offers Undo.

use std::collections::BTreeSet;

use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Subscription, WeakEntity};
use natamux_proto::workspaces::workspace::ChangeEntry;
use ui::{Checkbox, ToggleState, Tooltip, prelude::*};

use super::change_rules::status_tone;
use super::rollback_rules::{
    Tri, all_state, confirm_label, confirm_reason, first_checks, new_file_count, new_file_text,
    picked, row_checked, row_name, row_off, row_tip, still_changed, toggle, toggle_all,
};
use crate::app::dashboard::Dashboard;
use crate::components::colors::change_color;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::workspaces::workspace_rules::GitStep;
use crate::workspaces::workspace_view::WorkspaceView;

/// Opens the dialog for `files` of the workspace, Cancel focused. The dialog drops files that
/// stop being changed, and waits while the workspace runs another git step.
pub fn open(
    _dashboard: &Entity<Dashboard>,
    workspace: WeakEntity<WorkspaceView>,
    files: Vec<ChangeEntry>,
    window: &mut Window,
    cx: &mut App,
) {
    let view = cx.new(|cx| RollbackDialog::new(workspace, files, cx));
    let cancel = view.read(cx).cancel.clone();
    DialogLayer::open(view, window, cx);
    window.focus(&cancel, cx);
}

pub struct RollbackDialog {
    workspace: WeakEntity<WorkspaceView>,
    files: Vec<ChangeEntry>,
    checked: BTreeSet<String>,
    git_busy: bool,
    focus: FocusHandle,
    cancel: FocusHandle,
    submit: FocusHandle,
    _watch: Option<Subscription>,
}

impl RollbackDialog {
    fn new(
        workspace: WeakEntity<WorkspaceView>,
        files: Vec<ChangeEntry>,
        cx: &mut Context<Self>,
    ) -> Self {
        let watch = workspace.upgrade().map(|view| {
            cx.observe(&view, |this, view, cx| {
                this.sync(&view, cx);
            })
        });
        RollbackDialog {
            workspace,
            checked: first_checks(&files),
            files,
            git_busy: false,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
            submit: cx.focus_handle(),
            _watch: watch,
        }
    }

    /// Files that were rolled back or committed elsewhere leave the list.
    fn sync(&mut self, view: &Entity<WorkspaceView>, cx: &mut Context<Self>) {
        let view = view.read(cx);
        self.git_busy = view.git_pending().is_some();
        if let Some(list) = view.current_changes() {
            self.files = still_changed(&self.files, list);
            let files = &self.files;
            self.checked
                .retain(|path| files.iter().any(|file| &file.path == path));
        }
        cx.notify();
    }

    fn picked(&self) -> Vec<String> {
        picked(&self.files, &self.checked)
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let paths = self.picked();
        if confirm_reason(paths.len(), self.git_busy).is_some() {
            return;
        }
        cx.emit(DismissEvent);
        self.workspace
            .update(cx, |view, cx| {
                let step = GitStep::Rollback;
                let run = view.run_git(
                    step,
                    move |dashboard, id, cx| dashboard.rollback(id, paths, cx),
                    cx,
                );
                if let Some(answer) = run {
                    answer.detach();
                }
            })
            .ok();
    }
}

impl EventEmitter<DismissEvent> for RollbackDialog {}

impl Focusable for RollbackDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for RollbackDialog {
    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        vec![self.cancel.clone(), self.submit.clone()]
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.confirm(cx);
    }
}

fn toggle_state(tri: Tri) -> ToggleState {
    match tri {
        Tri::Checked => ToggleState::Selected,
        Tri::Mixed => ToggleState::Indeterminate,
        Tri::Unchecked => ToggleState::Unselected,
    }
}

impl RollbackDialog {
    fn file_row(&self, entry: &ChangeEntry, cx: &mut Context<Self>) -> AnyElement {
        let name = SharedString::from(row_name(entry));
        let path = entry.path.clone();
        let tone = change_color(status_tone(entry.status), cx);
        let boxed = Checkbox::new(name.clone(), row_checked(entry, &self.checked).into())
            .disabled(row_off(entry))
            .on_click(cx.listener(move |this, state: &ToggleState, _, cx| {
                this.checked = toggle(&this.checked, &path, state.selected());
                cx.notify();
            }));
        h_flex()
            .id(SharedString::from(format!("rollback-row-{}", entry.path)))
            .gap_2()
            .px_2()
            .h(px(32.))
            .flex_none()
            .items_center()
            .when_some(row_tip(entry), |this, tip| this.tooltip(Tooltip::text(tip)))
            .child(boxed)
            .child(
                Icon::new(IconName::File)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                div().min_w_0().flex_1().child(
                    Label::new(entry.path.clone())
                        .size(LabelSize::Small)
                        .buffer_font(cx)
                        .truncate(),
                ),
            )
            .child(
                Label::new(entry.status.letter())
                    .size(LabelSize::Small)
                    .color(Color::Custom(tone))
                    .buffer_font(cx),
            )
            .into_any_element()
    }

    fn file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();
        for entry in &self.files {
            rows.push(self.file_row(entry, cx));
        }
        v_flex()
            .gap_1p5()
            .child(
                Label::new("Files to roll back")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                v_flex()
                    .id("rollback-files")
                    .max_h(px(288.))
                    .overflow_y_scroll()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .bg(cx.theme().colors().editor_background)
                    .children(rows),
            )
    }
}

impl Render for RollbackDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let all = all_state(&self.files, &self.checked);
        let paths = self.picked();
        let reason = confirm_reason(paths.len(), self.git_busy);
        let all_box = Checkbox::new("rollback-all", toggle_state(all.tri))
            .label("All files")
            .disabled(all.off)
            .on_click(cx.listener(|this, _: &ToggleState, _, cx| {
                let tri = all_state(&this.files, &this.checked).tri;
                this.checked = toggle_all(&this.files, tri);
                cx.notify();
            }));
        let danger = new_file_text(new_file_count(&self.files, &self.checked));
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let submit = cx.listener(|this, _, _, cx| this.confirm(cx));
        let confirm = primary_button(
            confirm_label(paths.len()),
            &self.submit,
            true,
            reason.is_some(),
            submit,
        )
        .when_some(reason, |button, reason| {
            button.tooltip(Tooltip::text(reason))
        });
        let frame = DialogFrame::new("Roll back changes", close)
            .description("The checked files go back to how they are in the last commit.")
            .child(all_box)
            .child(self.file_list(cx))
            .children(
                danger.map(|text| Label::new(text).size(LabelSize::Small).color(Color::Error)),
            )
            .footer(cancel_button("Cancel", &self.cancel, cancel))
            .footer(confirm);
        div().track_focus(&self.focus).child(frame)
    }
}
