//! The edit form of the details panel (`features/tasks/TaskEditForm.tsx`): title, notes, priority,
//! labels and the tasks it waits for. Save (Mod+Enter from any field) makes `PATCH /tasks/:id`;
//! Cancel and a save close it.

use editor::Editor;
use gpui::{ClickEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent};
use natamux_proto::tasks::task::{TASK_PRIORITIES, Task, TaskChanges};
use ui::{ContextMenu, prelude::*};

use super::edit_rules::{parse_labels, parse_task_ids};
use crate::app::dashboard::Dashboard;
use crate::components::button_style::{action_button, key_hint, quiet_button};
use crate::components::form::{field, input_box, text_editor};
use crate::components::menu::MenuLayer;

/// The form is done: saved or cancelled.
pub struct Saved;

pub struct EditForm {
    task: Task,
    dashboard: Entity<Dashboard>,
    title: Entity<Editor>,
    notes: Entity<Editor>,
    labels: Entity<Editor>,
    depends_on: Entity<Editor>,
    priority: String,
    busy: bool,
    priority_focus: FocusHandle,
}

impl EventEmitter<Saved> for EditForm {}

fn filled(
    text: &str,
    placeholder: &str,
    lines: Option<(usize, usize)>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Editor> {
    let editor = text_editor(placeholder, lines, window, cx);
    editor.update(cx, |editor, cx| editor.set_text(text, window, cx));
    editor
}

impl EditForm {
    pub fn new(
        task: Task,
        dashboard: Entity<Dashboard>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title = filled(&task.title, "", None, window, cx);
        let notes = filled(&task.description, "", Some((8, 20)), window, cx);
        let labels = filled(&task.labels.join(", "), "", None, window, cx);
        let depends_on = filled(
            &task.depends_on.join(", "),
            "T-0001, T-0002",
            None,
            window,
            cx,
        );
        EditForm {
            priority: task.priority.clone(),
            task,
            dashboard,
            title,
            notes,
            labels,
            depends_on,
            busy: false,
            priority_focus: cx.focus_handle(),
        }
    }

    /// The title is focused and selected, as the web's form does on mount.
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.title.focus_handle(cx);
        window.focus(&handle, cx);
        self.title.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx)
        });
    }

    /// Title, notes, labels and the tasks it waits for, for the flow tests.
    #[cfg(test)]
    pub fn boxes(&self) -> Vec<Entity<Editor>> {
        vec![
            self.title.clone(),
            self.notes.clone(),
            self.labels.clone(),
            self.depends_on.clone(),
        ]
    }

    fn text(editor: &Entity<Editor>, cx: &App) -> String {
        editor.read(cx).text(cx)
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let title = Self::text(&self.title, cx).trim().to_owned();
        if title.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        let changes = TaskChanges {
            title: Some(title),
            description: Some(Self::text(&self.notes, cx)),
            priority: Some(self.priority.clone()),
            labels: Some(parse_labels(&Self::text(&self.labels, cx))),
            depends_on: Some(parse_task_ids(&Self::text(&self.depends_on, cx))),
        };
        let id = self.task.id.clone();
        let written = self
            .dashboard
            .update(cx, |dashboard, cx| dashboard.update_task(&id, changes, cx));
        cx.spawn(async move |this, cx| {
            let saved = written.await.is_ok();
            this.update(cx, |this, cx| {
                this.busy = false;
                if saved {
                    cx.emit(Saved);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn open_priority(&mut self, event: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.priority.clone();
        let this = cx.weak_entity();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            TASK_PRIORITIES.into_iter().fold(menu, |menu, priority| {
                let this = this.clone();
                menu.toggleable_entry(
                    capitalized(priority),
                    priority == picked,
                    IconPosition::Start,
                    None,
                    move |_, cx| {
                        this.update(cx, |this, cx| {
                            this.priority = priority.to_owned();
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

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl Render for EditForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blank = Self::text(&self.title, cx).trim().is_empty();
        let priority = Button::new("edit-priority", capitalized(&self.priority))
            .style(ButtonStyle::Outlined)
            .full_width()
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .track_focus(&self.priority_focus)
            .on_click(cx.listener(|this, event, window, cx| this.open_priority(event, window, cx)));
        let off = self.busy || blank;
        let save = action_button("edit-save", "Save")
            .key_binding(key_hint("mod-enter", off))
            .disabled(off)
            .on_click(cx.listener(|this, _, _, cx| this.save(cx)));
        v_flex()
            .id(SharedString::from(format!("edit-{}", self.task.id)))
            .key_context("NatamuxEdit")
            // Mod+Enter saves from any field.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let held = &event.keystroke.modifiers;
                if event.keystroke.key == "enter" && held.secondary() {
                    cx.stop_propagation();
                    this.save(cx);
                }
            }))
            .child(
                v_flex()
                    .p_3()
                    .gap_3()
                    .child(field("Title", input_box(&self.title, cx)))
                    .child(field("Notes (markdown)", input_box(&self.notes, cx)))
                    .child(field("Priority", priority))
                    .child(field(
                        "Labels, comma separated",
                        input_box(&self.labels, cx),
                    ))
                    .child(field(
                        "Waits for tasks (ids, comma separated)",
                        input_box(&self.depends_on, cx),
                    )),
            )
            .child(
                h_flex()
                    .min_h(rems_from_px(52.0_f32))
                    .flex_wrap()
                    .px_3()
                    .gap_2()
                    .justify_end()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        quiet_button("edit-cancel", "Cancel")
                            .key_binding(key_hint("escape", false))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Saved))),
                    )
                    .child(save),
            )
    }
}
