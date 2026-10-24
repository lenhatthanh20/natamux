//! The Add subtask dialog (`features/tasks/AddSubtaskDialog.tsx`, ui-spec 4.29): one Title field.
//! Enter and Mod+Enter add the subtask. On success it closes and toasts "Added T-5 to T-1"; on
//! an error the dashboard toasts the daemon's message and the dialog keeps the title.

use editor::{Editor, EditorEvent};
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Global, KeyDownEvent};
use gpui::{Subscription, WeakEntity};
use natamux_proto::tasks::task::{NewTaskInput, Task};
use ui::prelude::*;

use crate::app::dashboard::Dashboard;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogSize, DialogView, cancel_button, primary_button,
};
use crate::components::toast_rules::ToastTone;

struct GlobalDashboard(WeakEntity<Dashboard>);

impl Global for GlobalDashboard {}

/// Lets `open` find the window's dashboard. The task view calls it when it is made.
pub fn init(dashboard: &Entity<Dashboard>, cx: &mut App) {
    cx.set_global(GlobalDashboard(dashboard.downgrade()));
}

/// Opens the dialog for a subtask of `parent`, for the sidebar and the table row menus.
pub fn open(parent: Task, window: &mut Window, cx: &mut App) {
    let Some(dashboard) = cx
        .try_global::<GlobalDashboard>()
        .and_then(|global| global.0.upgrade())
    else {
        return;
    };
    let dialog = cx.new(|cx| AddSubtaskDialog::new(parent, dashboard, window, cx));
    let field = dialog.read(cx).title.focus_handle(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&field, cx);
}

pub struct AddSubtaskDialog {
    parent: Task,
    dashboard: Entity<Dashboard>,
    title: Entity<Editor>,
    busy: bool,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
    _edits: Subscription,
}

impl AddSubtaskDialog {
    fn new(
        parent: Task,
        dashboard: Entity<Dashboard>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| Editor::single_line(window, cx));
        let edits = cx.subscribe(&title, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        });
        AddSubtaskDialog {
            parent,
            dashboard,
            title,
            busy: false,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
            _edits: edits,
        }
    }

    fn typed(&self, cx: &App) -> String {
        self.title.read(cx).text(cx).trim().to_string()
    }
}

impl EventEmitter<DismissEvent> for AddSubtaskDialog {}

impl Focusable for AddSubtaskDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for AddSubtaskDialog {
    fn size(&self) -> DialogSize {
        DialogSize::Small
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        vec![
            self.title.focus_handle(cx),
            self.cancel_focus.clone(),
            self.submit_focus.clone(),
        ]
    }

    fn busy(&self, _: &App) -> bool {
        self.busy
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let title = self.typed(cx);
        if title.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        let parent = self.parent.id.clone();
        let input = NewTaskInput {
            title,
            parent_id: Some(parent.clone()),
            repo: None,
        };
        let dashboard = self.dashboard.clone();
        // The web merges the subtask and keeps the selection; `create_task` selects the new
        // task, so the old selection comes back after it.
        let selected = dashboard.read(cx).selected.clone();
        let written = dashboard.update(cx, |dashboard, cx| dashboard.create_task(input, None, cx));
        cx.spawn(async move |this, cx| {
            let result = written.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                if let Ok(task) = result {
                    this.dashboard.update(cx, |dashboard, cx| {
                        dashboard.select(selected, cx);
                        let text = format!("Added {} to {parent}", task.id);
                        dashboard.toast(&text, ToastTone::Info, None, cx);
                    });
                    cx.emit(DismissEvent);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

impl Render for AddSubtaskDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let empty = self.typed(cx).is_empty();
        let field = v_flex()
            .gap_1()
            .child(
                Label::new("Title")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                div()
                    .h(rems_from_px(32.0_f32))
                    .px(rems_from_px(10.0_f32))
                    .flex()
                    .items_center()
                    .rounded(rems_from_px(6.0_f32))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.editor_background)
                    // Enter adds too, as the web's form does; Mod+Enter is the dialog layer's.
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        let held = &event.keystroke.modifiers;
                        if event.keystroke.key == "enter" && !held.modified() {
                            cx.stop_propagation();
                            this.submit(window, cx);
                        }
                    }))
                    .child(self.title.clone()),
            );
        let frame = DialogFrame::new(
            format!("Add a subtask to {}", self.parent.id),
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .footer(cancel_button(
            "Cancel",
            &self.cancel_focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ))
        .footer(primary_button(
            "Add subtask",
            &self.submit_focus,
            empty || self.busy,
            cx.listener(|this, _, window, cx| this.submit(window, cx)),
        ))
        .child(field);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
