//! The Add task dialog (F4): a backlog task with a title, a project when there are several, and a
//! template when templates exist. Enter adds it. Mod+Enter (or the Start a task button) closes and
//! opens the Start a task dialog with the typed title.

use editor::{Editor, EditorEvent};
use gpui::{
    ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent,
    Subscription,
};
use natamux_proto::tasks::task::NewTaskInput;
use natamux_proto::tasks::template::TaskTemplate;
use ui::{ButtonLike, ContextMenu, TintColor, Tooltip, prelude::*};

use super::task_view_rules::{add_blocked_reason, task_repo};
use crate::app::dashboard::Dashboard;
use crate::components::chips::Kbd;
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView};
use crate::components::menu::MenuLayer;
use crate::workspaces::new_workspace_dialog;

/// Opens the dialog with the title focused, and `title` typed in it when given.
pub fn open(
    dashboard: &Entity<Dashboard>,
    title: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    let dialog = cx.new(|cx| AddTaskDialog::new(dashboard, title, window, cx));
    let field = dialog.read(cx).title.focus_handle(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&field, cx);
}

pub struct AddTaskDialog {
    dashboard: Entity<Dashboard>,
    title: Entity<Editor>,
    /// The project picked in the select; `task_repo` falls back to the first.
    repo: Option<String>,
    template: Option<String>,
    templates: Vec<TaskTemplate>,
    busy: bool,
    focus: FocusHandle,
    project_focus: FocusHandle,
    template_focus: FocusHandle,
    start_focus: FocusHandle,
    add_focus: FocusHandle,
    _edits: Subscription,
}

impl AddTaskDialog {
    fn new(
        dashboard: Entity<Dashboard>,
        title: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Title", window, cx);
            if let Some(title) = title {
                editor.set_text(title, window, cx);
            }
            editor
        });
        let edits = cx.subscribe(&field, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        });
        let repo = dashboard.read(cx).last_add_repo.clone();
        let dialog = AddTaskDialog {
            dashboard,
            title: field,
            repo,
            template: None,
            templates: Vec::new(),
            busy: false,
            focus: cx.focus_handle(),
            project_focus: cx.focus_handle(),
            template_focus: cx.focus_handle(),
            start_focus: cx.focus_handle(),
            add_focus: cx.focus_handle(),
            _edits: edits,
        };
        dialog.read_templates(cx);
        dialog
    }

    /// Templates change only in project files, so they are read once.
    fn read_templates(&self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.list_templates();
        cx.spawn(async move |this, cx| {
            if let Ok(templates) = answer.await {
                this.update(cx, |this, cx| {
                    this.templates = templates;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn typed(&self, cx: &App) -> String {
        self.title.read(cx).text(cx).trim().to_string()
    }

    fn no_projects(&self, cx: &App) -> bool {
        let data = &self.dashboard.read(cx).data;
        data.loaded && data.repos.is_empty()
    }

    fn blocked_reason(&self, cx: &App) -> Option<&'static str> {
        add_blocked_reason(self.no_projects(cx), &self.typed(cx))
    }

    fn add(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.blocked_reason(cx).is_some() {
            return;
        }
        let title = self.typed(cx);
        let repo = task_repo(&self.dashboard.read(cx).data.repos, self.repo.as_deref());
        let input = NewTaskInput {
            title,
            parent_id: None,
            repo: repo.clone(),
        };
        let template = self.template.clone();
        self.busy = true;
        let written = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.last_add_repo = repo;
            dashboard.create_task(input, template, cx)
        });
        cx.spawn(async move |this, cx| {
            let result = written.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                // On an error the dashboard toasted it; the dialog stays with the title.
                if result.is_ok() {
                    cx.emit(DismissEvent);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Mod+Enter and the Start a task button.
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.typed(cx);
        let title = (!typed.is_empty()).then_some(typed);
        let dashboard = self.dashboard.clone();
        cx.emit(DismissEvent);
        window.defer(cx, move |window, cx| {
            new_workspace_dialog::open_with(&dashboard, None, title, window, cx);
        });
    }

    fn open_select(
        &self,
        position: gpui::Point<Pixels>,
        repos: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity().downgrade();
        let menu = if repos {
            let picked = task_repo(&self.dashboard.read(cx).data.repos, self.repo.as_deref());
            let repos = self.dashboard.read(cx).data.repos.clone();
            ContextMenu::build(window, cx, move |mut menu, _, _| {
                for repo in repos {
                    let this = this.clone();
                    let id = repo.id.clone();
                    let checked = picked.as_deref() == Some(id.as_str());
                    menu = menu.toggleable_entry(
                        repo.name,
                        checked,
                        IconPosition::Start,
                        None,
                        move |_, cx| {
                            this.update(cx, |this, cx| {
                                this.repo = Some(id.clone());
                                cx.notify();
                            })
                            .ok();
                        },
                    );
                }
                menu
            })
        } else {
            let picked = self.template.clone();
            let templates = self.templates.clone();
            ContextMenu::build(window, cx, move |menu, _, _| {
                let none = this.clone();
                let menu = menu.toggleable_entry(
                    "No template",
                    picked.is_none(),
                    IconPosition::Start,
                    None,
                    move |_, cx| {
                        none.update(cx, |this, cx| {
                            this.template = None;
                            cx.notify();
                        })
                        .ok();
                    },
                );
                templates.into_iter().fold(menu, |menu, template| {
                    let this = this.clone();
                    let checked = picked.as_deref() == Some(template.id.as_str());
                    menu.toggleable_entry(
                        template.name,
                        checked,
                        IconPosition::Start,
                        None,
                        move |_, cx| {
                            this.update(cx, |this, cx| {
                                this.template = Some(template.id.clone());
                                cx.notify();
                            })
                            .ok();
                        },
                    )
                })
            })
        };
        MenuLayer::open(menu, position, window, cx);
    }

    fn select_button(&self, repos: bool, cx: &mut Context<Self>) -> Button {
        let data = &self.dashboard.read(cx).data;
        let (id, label, name, focus) = if repos {
            let picked = task_repo(&data.repos, self.repo.as_deref());
            let name = data
                .repos
                .iter()
                .find(|repo| Some(&repo.id) == picked.as_ref())
                .map_or_else(String::new, |repo| repo.name.clone());
            ("add-task-project", "Project", name, &self.project_focus)
        } else {
            let name = self
                .templates
                .iter()
                .find(|template| Some(&template.id) == self.template.as_ref())
                .map_or_else(|| "No template".to_string(), |t| t.name.clone());
            ("add-task-template", "Template", name, &self.template_focus)
        };
        Button::new(id, name)
            .style(ButtonStyle::Outlined)
            .track_focus(focus)
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .tooltip(Tooltip::text(label))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_select(event.position(), repos, window, cx);
            }))
    }
}

impl EventEmitter<DismissEvent> for AddTaskDialog {}

impl Focusable for AddTaskDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for AddTaskDialog {
    fn size(&self) -> DialogSize {
        DialogSize::Small
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let data = &self.dashboard.read(cx).data;
        let mut ring = vec![self.title.focus_handle(cx)];
        if data.repos.len() > 1 {
            ring.push(self.project_focus.clone());
        }
        if !self.templates.is_empty() {
            ring.push(self.template_focus.clone());
        }
        ring.push(self.start_focus.clone());
        ring.push(self.add_focus.clone());
        ring
    }

    fn busy(&self, _: &App) -> bool {
        self.busy
    }

    /// Mod+Enter: Start a task.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start(window, cx);
    }
}

impl Render for AddTaskDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let blocked = self.blocked_reason(cx);
        let several_repos = self.dashboard.read(cx).data.repos.len() > 1;
        let has_templates = !self.templates.is_empty();
        let field = div()
            .h(rems_from_px(32.0_f32))
            .px(rems_from_px(10.0_f32))
            .flex()
            .items_center()
            .rounded(rems_from_px(6.0_f32))
            .border_1()
            .border_color(colors.border)
            .bg(colors.editor_background)
            // Enter adds; Mod+Enter is the dialog layer's.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" && !event.keystroke.modifiers.modified() {
                    cx.stop_propagation();
                    this.add(cx);
                }
            }))
            .child(self.title.clone());
        let selects = (several_repos || has_templates).then(|| {
            h_flex()
                .gap_2()
                .when(several_repos, |row| row.child(self.select_button(true, cx)))
                .when(has_templates, |row| {
                    row.child(self.select_button(false, cx))
                })
        });
        let start = ButtonLike::new("add-task-start")
            .style(ButtonStyle::Outlined)
            .track_focus(&self.start_focus)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start(window, cx)))
            .child(
                h_flex()
                    .gap_2()
                    .px_1()
                    .child(Label::new("Start a task"))
                    .child(Kbd::new("mod-enter")),
            );
        let add = ButtonLike::new("add-task-submit")
            .style(ButtonStyle::Tinted(TintColor::Accent))
            .track_focus(&self.add_focus)
            .disabled(blocked.is_some())
            .when_some(blocked, |button, reason| {
                button.tooltip(Tooltip::text(reason))
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.add(cx)))
            .child(
                h_flex()
                    .gap_2()
                    .px_1()
                    .child(Label::new("Add"))
                    .child(Kbd::new("enter")),
            );
        let mut frame = DialogFrame::new(
            "Add a task",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        if self.no_projects(cx) {
            frame = frame.footer_note("Add a project first");
        }
        let frame = frame
            .footer(start)
            .footer(add)
            .child(field)
            .children(selects);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
