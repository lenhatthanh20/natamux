//! The Run more tries dialog (docs/ui-spec.md 4.14, `features/tasks/FanOutForm.tsx`): how many
//! copies, and whether Claude Code starts in each. Run tries (Mod+Enter) closes it and makes
//! `POST /tasks/:id/fan-out`.

use gpui::{ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable};
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::MAX_TRIES;
use ui::{ButtonLike, Checkbox, ContextMenu, prelude::*};

use crate::app::dashboard::Dashboard;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::menu::MenuLayer;

/// `FanOutForm.tsx`: two more tries, each with its agent, unless the user picks otherwise.
const DEFAULT_COUNT: u32 = 2;

pub fn open(dashboard: &Entity<Dashboard>, task: &Task, window: &mut Window, cx: &mut App) {
    let dialog = cx.new(|cx| FanOutDialog {
        dashboard: dashboard.clone(),
        task_id: task.id.clone(),
        count: DEFAULT_COUNT,
        start: true,
        focus: cx.focus_handle(),
        count_focus: cx.focus_handle(),
        start_focus: cx.focus_handle(),
        cancel_focus: cx.focus_handle(),
        submit_focus: cx.focus_handle(),
    });
    let first = dialog.read(cx).count_focus.clone();
    DialogLayer::open(dialog, window, cx);
    window.focus(&first, cx);
}

pub struct FanOutDialog {
    dashboard: Entity<Dashboard>,
    task_id: String,
    count: u32,
    start: bool,
    focus: FocusHandle,
    count_focus: FocusHandle,
    start_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
}

impl FanOutDialog {
    fn open_count(&mut self, event: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.count;
        let this = cx.weak_entity();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            (1..=MAX_TRIES).fold(menu, |menu, count| {
                let this = this.clone();
                menu.toggleable_entry(
                    count.to_string(),
                    count == picked,
                    IconPosition::Start,
                    None,
                    move |_, cx| {
                        this.update(cx, |this, cx| {
                            this.count = count;
                            cx.notify();
                        })
                        .ok();
                    },
                )
            })
        });
        MenuLayer::open(menu, event.position(), window, cx);
    }
}

impl EventEmitter<DismissEvent> for FanOutDialog {}

impl Focusable for FanOutDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for FanOutDialog {
    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        vec![
            self.count_focus.clone(),
            self.start_focus.clone(),
            self.cancel_focus.clone(),
            self.submit_focus.clone(),
        ]
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (id, count, start) = (self.task_id.clone(), self.count, self.start);
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.fan_out(&id, count, start, cx).detach()
        });
        cx.emit(DismissEvent);
    }
}

impl Render for FanOutDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tries = if self.count == 1 {
            "more try"
        } else {
            "more tries"
        };
        let count = Button::new("fan-out-count", self.count.to_string())
            .style(ButtonStyle::Outlined)
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .track_focus(&self.count_focus)
            .on_click(cx.listener(|this, event, window, cx| this.open_count(event, window, cx)));
        let start = ButtonLike::new("fan-out-start")
            .track_focus(&self.start_focus)
            .on_click(cx.listener(|this, _, _, cx| {
                this.start = !this.start;
                cx.notify();
            }))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Checkbox::new("fan-out-start-box", self.start.into())
                            .visualization_only(true),
                    )
                    .child(Label::new("Start Claude Code in each")),
            );
        let frame = DialogFrame::new(
            "Run more tries",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .description(
            "Each try is a copy of this task in its own workspace, so the agents do not meet.",
        )
        .footer(cancel_button(
            "Cancel",
            &self.cancel_focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ))
        .footer(primary_button(
            "Run tries",
            &self.submit_focus,
            false,
            false,
            cx.listener(|this, _, window, cx| this.submit(window, cx)),
        ))
        .child(
            h_flex()
                .gap_2()
                .child(Label::new("Run"))
                .child(count)
                .child(Label::new(tries)),
        )
        .child(start);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
