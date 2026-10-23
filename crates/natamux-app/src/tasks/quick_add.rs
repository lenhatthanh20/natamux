//! The quick add under the view header (ui-spec 4.3, the form of `Dashboard.tsx`): the backlog
//! field (`n`), the project select when there are several projects, the template select when
//! templates exist, Add task, and Start a task.

use editor::{Editor, EditorEvent};
use gpui::{ClickEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent, Subscription};
use natamux_proto::tasks::task::NewTaskInput;
use natamux_proto::tasks::template::TaskTemplate;
use ui::{ButtonLike, ContextMenu, TintColor, Tooltip, prelude::*};

use super::task_view_rules::{add_blocked_reason, task_repo};
use crate::app::commands::shortcut_of;
use crate::app::dashboard::Dashboard;
use crate::components::chips::Kbd;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

/// Start a task was asked for, with the typed title when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartTask(pub Option<String>);

pub struct QuickAdd {
    dashboard: Entity<Dashboard>,
    title: Entity<Editor>,
    /// The project picked in the select; `task_repo` falls back to the first.
    repo: Option<String>,
    template: Option<String>,
    templates: Vec<TaskTemplate>,
    _edits: Subscription,
}

impl EventEmitter<StartTask> for QuickAdd {}

impl QuickAdd {
    pub fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Add a task to the backlog (n)", window, cx);
            editor
        });
        let edits = cx.subscribe(&title, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        });
        let quick_add = QuickAdd {
            dashboard,
            title,
            repo: None,
            template: None,
            templates: Vec::new(),
            _edits: edits,
        };
        quick_add.read_templates(cx);
        quick_add
    }

    /// Templates change only in project files, so they are read once (the web's query).
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

    /// `n`: the backlog field.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.title.focus_handle(cx), cx);
    }

    /// Puts `text` in the field, as typing it would (the `add-task-typed` shot).
    pub fn set_title(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.title
            .update(cx, |editor, cx| editor.set_text(text, window, cx));
    }

    /// Start a task used up the typed title.
    pub fn clear(&self, window: &mut Window, cx: &mut App) {
        self.title.update(cx, |editor, cx| editor.clear(window, cx));
    }

    /// What is typed, for Start a task.
    /// The project a new task goes to: Import issues reads that project's issues (`taskRepo`).
    pub fn repo(&self, cx: &App) -> Option<String> {
        task_repo(&self.dashboard.read(cx).data.repos, self.repo.as_deref())
    }

    pub fn typed_title(&self, cx: &App) -> Option<String> {
        let title = self.title.read(cx).text(cx).trim().to_string();
        (!title.is_empty()).then_some(title)
    }

    fn blocked_reason(&self, cx: &App) -> Option<&'static str> {
        let data = &self.dashboard.read(cx).data;
        let no_projects = data.loaded && data.repos.is_empty();
        add_blocked_reason(no_projects, &self.title.read(cx).text(cx))
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.blocked_reason(cx).is_some() {
            return;
        }
        let Some(title) = self.typed_title(cx) else {
            return;
        };
        let repo = task_repo(&self.dashboard.read(cx).data.repos, self.repo.as_deref());
        let input = NewTaskInput {
            title: title.clone(),
            parent_id: None,
            repo,
        };
        let template = self.template.clone();
        // Cleared now, not when the daemon answers: the next title may be typed meanwhile.
        self.title.update(cx, |editor, cx| editor.clear(window, cx));
        let written = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.create_task(input, template, cx)
        });
        let editor = self.title.downgrade();
        cx.spawn_in(window, async move |_, cx| {
            if written.await.is_err() {
                // The dashboard toasted the error; the title comes back unless a new one is typed.
                editor
                    .update_in(cx, |editor, window, cx| {
                        if editor.is_empty(cx) {
                            editor.set_text(title, window, cx);
                        }
                    })
                    .ok();
            }
        })
        .detach();
    }

    fn start_task(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(StartTask(self.typed_title(cx)));
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
        let (id, label, name) = if repos {
            let picked = task_repo(&data.repos, self.repo.as_deref());
            let name = data
                .repos
                .iter()
                .find(|repo| Some(&repo.id) == picked.as_ref())
                .map_or_else(String::new, |repo| repo.name.clone());
            ("quick-add-project", "Project", name)
        } else {
            let name = self
                .templates
                .iter()
                .find(|template| Some(&template.id) == self.template.as_ref())
                .map_or_else(|| "No template".to_string(), |t| t.name.clone());
            ("quick-add-template", "Template", name)
        };
        Button::new(id, name)
            .style(ButtonStyle::Outlined)
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .tooltip(Tooltip::text(label))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_select(event.position(), repos, window, cx);
            }))
    }
}

impl Render for QuickAdd {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let blocked = self.blocked_reason(cx);
        let data = &self.dashboard.read(cx).data;
        let several_repos = data.repos.len() > 1;
        let has_tasks = !data.tasks.is_empty();
        let has_templates = !self.templates.is_empty();
        let typed = self.typed_title(cx).is_some();
        let field = div()
            .flex_1()
            .min_w(rems_from_px(256.0_f32))
            .h(rems_from_px(28.0_f32))
            .px(rems_from_px(10.0_f32))
            .flex()
            .items_center()
            .rounded(rems_from_px(6.0_f32))
            .border_1()
            .border_color(colors.border)
            .bg(colors.editor_background)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && !event.keystroke.modifiers.modified() {
                    cx.stop_propagation();
                    this.submit(window, cx);
                }
            }))
            .child(self.title.clone());
        // Not busy while a task is added: the next title is often typed before the answer.
        let add = Button::new("quick-add-submit", "Add task")
            .start_icon(Icon::from_path(icon_path("plus")).size(IconSize::Small))
            .style(ButtonStyle::Filled)
            .disabled(blocked.is_some())
            .when_some(blocked, |button, reason| {
                button.tooltip(Tooltip::text(reason))
            })
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));
        let start_keys = shortcut_of("workspace.new").unwrap_or_default();
        let start = ButtonLike::new("quick-add-start")
            .style(ButtonStyle::Tinted(TintColor::Accent))
            .when(typed, |button| {
                button.tooltip(Tooltip::text("Start a task with this title"))
            })
            .on_click(cx.listener(Self::start_task))
            .child(
                h_flex()
                    .gap_2()
                    .px_1()
                    .child(Icon::from_path(icon_path("square-terminal")).size(IconSize::Small))
                    .child(Label::new("Start a task"))
                    .child(Kbd::new(start_keys)),
            );
        h_flex()
            .flex_none()
            .flex_wrap()
            .gap_2()
            .px_4()
            .pt_4()
            .pb_3()
            .child(field)
            .when(several_repos, |row| row.child(self.select_button(true, cx)))
            .when(has_templates, |row| {
                row.child(self.select_button(false, cx))
            })
            .child(add)
            // Starting an agent is the main daily action; the empty state offers it already.
            .when(has_tasks, |row| row.child(start))
    }
}

impl Focusable for QuickAdd {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.title.focus_handle(cx)
    }
}
