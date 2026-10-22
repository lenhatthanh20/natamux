//! The fields of the Start a task dialog, in two groups: "Context" (title, source, skill, notes or
//! reference, attachments) and "Workspace and agent" (project, branch, agent).

use std::path::PathBuf;

use editor::Editor;
use gpui::{ClickEvent, Entity, FontWeight, PathPromptOptions};
use ui::prelude::*;

use super::super::new_workspace_rules::{PickedFile, SourceChoice, fit_attachments, format_bytes};
use super::{NewWorkspaceDialog, Pick};
use crate::agents::agent_choices::agent_label;

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            Label::new(label)
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Muted),
        )
        .child(control)
}

fn hint(text: impl Into<SharedString>) -> impl IntoElement {
    Label::new(text).size(LabelSize::Small).color(Color::Muted)
}

fn section_title(text: &'static str) -> impl IntoElement {
    Label::new(text).weight(FontWeight::SEMIBOLD)
}

fn input_box(editor: &Entity<Editor>, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    div()
        .px_2p5()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(editor.clone())
}

impl NewWorkspaceDialog {
    /// A select: a button with the picked option that opens a menu of the others.
    fn select(
        &self,
        id: &'static str,
        pick: Pick,
        text: impl Into<SharedString>,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let focus = match pick {
            Pick::Source => &self.stops.source,
            Pick::Skill => &self.stops.skill,
            Pick::Project => &self.stops.project,
            Pick::Branch => &self.stops.branch,
            Pick::Agent => &self.stops.agent,
        };
        Button::new(id, text)
            .style(ButtonStyle::Outlined)
            .full_width()
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .track_focus(focus)
            .disabled(disabled)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // At the pointer for a mouse click, under the button for Enter or Space.
                this.open_menu(pick, event.position(), window, cx)
            }))
    }

    pub(super) fn render_context(
        &self,
        choice: &SourceChoice,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let choices = self.choices();
        let manual = choice.kind() == "manual";
        let source = (choices.len() > 1).then(|| {
            field(
                "Context from",
                self.select("new-task-source", Pick::Source, choice.label(), false, cx),
            )
        });
        let skill = (*choice == SourceChoice::AgentSkill).then(|| {
            let text = if self.skill.is_empty() {
                "Pick a skill".to_string()
            } else {
                self.skill.clone()
            };
            let button = self
                .select("new-task-skill", Pick::Skill, text, false, cx)
                .start_icon(Icon::from_path("icons/lucide/sparkles.svg").size(IconSize::Small));
            field("Skill", button)
        });
        let text = if manual {
            field("Notes", input_box(&self.notes, cx)).into_any_element()
        } else {
            v_flex()
                .gap_1()
                .child(field("Reference", input_box(&self.reference, cx)))
                .child(hint(choice.ref_hint(&self.skill)))
                .into_any_element()
        };
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(section_title("Context"))
                    .child(hint(choice.context_hint(&self.skill))),
            )
            .child(field("Title", input_box(&self.title, cx)))
            .children(source)
            .children(skill)
            .child(text)
            .child(field("Attachments", self.render_files(cx)))
    }

    fn render_files(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let add = Button::new("new-task-files", "Add files")
            .style(ButtonStyle::Outlined)
            .size(ButtonSize::Compact)
            .start_icon(Icon::from_path("icons/lucide/paperclip.svg").size(IconSize::Small))
            .track_focus(&self.stops.files)
            .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx)));
        let picker = h_flex()
            .flex_wrap()
            .gap_2()
            .px_2p5()
            .py_2()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(cx.theme().colors().border)
            .child(add)
            .child(hint(
                "Or drop files here, such as screenshots, logs or specs. They are part of the \
                 context.",
            ));
        let rows = self.files.iter().map(|file| {
            let id = file.id;
            h_flex()
                .gap_2()
                .child(
                    div().min_w_0().flex_1().child(
                        Label::new(file.name.clone())
                            .size(LabelSize::Small)
                            .buffer_font(cx)
                            .truncate(),
                    ),
                )
                .child(hint(format_bytes(file.size)))
                .child(
                    IconButton::new(("remove-file", id), IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .tooltip(ui::Tooltip::text(format!("Remove {}", file.name)))
                        .on_click(cx.listener(move |this, _, _, cx| this.remove_file(id, cx))),
                )
        });
        let refused = self.refused.iter().map(|reason| {
            Label::new(reason.clone())
                .size(LabelSize::Small)
                .color(Color::Error)
        });
        v_flex()
            .gap_1()
            .child(picker)
            .children(rows)
            .children(refused)
    }

    pub(super) fn render_setup(&self, branch: String, cx: &mut Context<Self>) -> impl IntoElement {
        let repo = self.repo(cx);
        let repo_name = self
            .dashboard
            .read(cx)
            .data
            .repos
            .iter()
            .find(|each| each.id == repo)
            .map_or_else(|| repo.clone(), |each| each.name.clone());
        let agent = if self.agent.is_empty() {
            "No agent, a plain terminal".to_string()
        } else {
            agent_label(Some(&self.agent))
        };
        let project = self.select("new-task-project", Pick::Project, repo_name, false, cx);
        let branch = self.select(
            "new-task-branch",
            Pick::Branch,
            branch,
            self.branches.is_none(),
            cx,
        );
        let agent = self.select("new-task-agent", Pick::Agent, agent, false, cx);
        v_flex()
            .gap_3()
            .child(section_title("Workspace and agent"))
            .child(field("Project", project))
            .child(field("Branch from", branch))
            .child(field("Agent", agent))
    }

    pub(super) fn pick_files(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                this.update(cx, |this, cx| this.add_paths(paths, cx)).ok();
            }
        })
        .detach();
    }

    /// Files from the picker or dropped on the dialog. Their sizes are read off the main thread;
    /// a file that cannot be read fails the start with its name.
    pub(super) fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let sizes = cx.background_executor().spawn(async move {
            paths
                .into_iter()
                .map(|path| {
                    let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                    (name, size, path)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let sizes = sizes.await;
            this.update(cx, |this, cx| {
                let added = sizes
                    .into_iter()
                    .map(|(name, size, path)| {
                        this.next_file += 1;
                        PickedFile {
                            id: this.next_file,
                            name,
                            size,
                            path,
                        }
                    })
                    .collect();
                let (kept, refused) = fit_attachments(&this.files, added);
                this.files.extend(kept);
                this.refused = refused;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_file(&mut self, id: u64, cx: &mut Context<Self>) {
        self.files.retain(|file| file.id != id);
        self.refused.clear();
        cx.notify();
    }
}
