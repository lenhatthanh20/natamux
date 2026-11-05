//! The cards of the Start a task dialog (T-0292, option B): Task (title, source, skill, notes or
//! reference, attachments and their file rows), Workspace (project, worktree, branch) and Agent
//! (agent, task flow). Each select opens its menu (`new_workspace_menus`).

use std::path::PathBuf;

use gpui::{AnyElement, ClickEvent, ExternalPaths, KeyDownEvent, PathPromptOptions};
use ui::{ButtonLike, Switch, ToggleState, Tooltip, prelude::*};

use super::super::new_workspace_rules::{
    PickedFile, SourceChoice, fit_attachments, format_bytes, prefill_branch,
};
use super::super::new_workspace_view_rules::{
    CHOOSE_AGENT, ErrorRow, PICK_AGENT, branch_help, shows_default, source_icon, worktree_help,
    worktree_tip,
};
use super::{NewWorkspaceDialog, Pick};
use crate::agents::agent_choices::agent_label;
use crate::agents::limits_rules::{current, picker_note};
use crate::app::clock::now_ms;
use crate::components::button_style::{action_button, button_icon};
use crate::components::colors::accent;
use crate::components::form::SettingsInput;
use crate::components::segmented::Segmented;
use crate::components::settings_card::{
    SeparatorInset, SettingsCard, SettingsRow, error_line, foot_text,
};
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::components::text_size::row_text;

/// Every select of the dialog has this width, not the width of its text.
const SELECT_WIDTH: f32 = 240.;
/// The Title and Branch inputs.
const INPUT_WIDTH: f32 = 340.;
const ATTACH_HELP: &str =
    "Or drop files here, such as screenshots, logs or specs. They are part of the context.";

/// The 12 px icon before a select's text.
fn lead_icon(name: &str) -> AnyElement {
    button_icon(name)
        .size(IconSize::XSmall)
        .color(Color::Placeholder)
        .into_any_element()
}

/// A select's text, cut with an ellipsis, and a note after it in the placeholder colour.
fn select_text(text: impl Into<SharedString>, color: Color, note: Option<String>) -> AnyElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap(rems_from_px(4.0_f32))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .child(Label::new(text).size(row_text()).color(color).truncate()),
        )
        .children(note.map(|note| {
            div()
                .flex_none()
                .child(Label::new(note).size(row_text()).color(Color::Placeholder))
        }))
        .into_any_element()
}

/// A row whose control takes the next line at full width (Notes, Reference): the label, an
/// optional word at the right, the control, then a hint.
fn stack_row(
    label: &'static str,
    end: Option<&'static str>,
    control: impl IntoElement,
    hint: Option<String>,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap(rems_from_px(6.0_f32))
        .px(rems_from_px(12.0_f32))
        .pt(rems_from_px(8.0_f32))
        .pb(rems_from_px(10.0_f32))
        .child(
            h_flex()
                .w_full()
                .gap(rems_from_px(8.0_f32))
                .child(Label::new(label).size(row_text()))
                .child(div().flex_1())
                .children(end.map(|end| {
                    Label::new(end)
                        .size(LabelSize::Small)
                        .color(Color::Placeholder)
                })),
        )
        .child(control)
        .children(hint.map(foot_text))
        .into_any_element()
}

/// Files dragged over the Task card turn its border dashed in the accent colour. The frame lies
/// over the card and takes no click; the drop itself is handled by the whole body.
fn drop_frame(card: impl IntoElement, cx: &App) -> AnyElement {
    let accent = accent(cx);
    div()
        .relative()
        .w_full()
        .child(card)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(rems_from_px(8.0_f32))
                .border_1()
                .border_dashed()
                .border_color(gpui::transparent_black())
                .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(accent)),
        )
        .into_any_element()
}

impl NewWorkspaceDialog {
    /// A select: the lead icon, the picked option and a chevron; a click opens its menu.
    fn select(
        &self,
        id: &'static str,
        pick: Pick,
        lead: AnyElement,
        text: AnyElement,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focus = match pick {
            Pick::Source => &self.stops.source,
            Pick::Skill => &self.stops.skill,
            Pick::Project => &self.stops.project,
            Pick::Branch => &self.stops.branch,
            Pick::Agent => &self.stops.agent,
        };
        let button = ButtonLike::new(id)
            .style(ButtonStyle::Outlined)
            .full_width()
            .height(rems_from_px(24.0_f32).into())
            .track_focus(focus)
            .disabled(disabled)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // At the pointer for a mouse click, under the button for Enter or Space.
                this.open_menu(pick, event.position(), window, cx)
            }))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .pl(rems_from_px(8.0_f32))
                    .pr(rems_from_px(6.0_f32))
                    .gap(rems_from_px(6.0_f32))
                    .text_left()
                    .child(lead)
                    .child(text)
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    ),
            );
        div()
            .flex_none()
            .w(rems_from_px(SELECT_WIDTH))
            .when(disabled, |select| select.opacity(0.45))
            .child(button)
            .into_any_element()
    }

    pub(super) fn task_card(
        &self,
        choice: &SourceChoice,
        error_row: Option<ErrorRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = SettingsRow::new("new-task-title", "Title")
            .control(SettingsInput::new(&self.title).width(INPUT_WIDTH));
        let mut rows = vec![title.into_any_element()];
        if self.choices().len() > 1 {
            let lead = lead_icon(source_icon(choice));
            let text = select_text(choice.label(), Color::Default, None);
            let select = self.select("new-task-source", Pick::Source, lead, text, false, cx);
            let row = SettingsRow::new("new-task-source-row", "Context from").control(select);
            rows.push(row.into_any_element());
        }
        if *choice == SourceChoice::AgentSkill {
            let text = if self.skill.is_empty() {
                select_text("Pick a skill", Color::Placeholder, None)
            } else {
                select_text(self.skill.clone(), Color::Default, None)
            };
            let lead = lead_icon("sparkles");
            let select = self.select("new-task-skill", Pick::Skill, lead, text, false, cx);
            let row = SettingsRow::new("new-task-skill-row", "Skill")
                .error((error_row == Some(ErrorRow::Skill)).then_some(PICK_AGENT))
                .control(select);
            rows.push(row.into_any_element());
        }
        rows.push(if choice.kind() == "manual" {
            stack_row(
                "Notes",
                Some("Optional"),
                SettingsInput::new(&self.notes).area(),
                None,
            )
        } else {
            stack_row(
                "Reference",
                None,
                SettingsInput::new(&self.reference),
                Some(choice.ref_hint(&self.skill)),
            )
        });
        rows.push(self.attachments_row(cx));
        let files: Vec<AnyElement> = self
            .files
            .iter()
            .map(|file| self.file_row(file, cx))
            .collect();
        rows.extend(files);
        drop_frame(SettingsCard::new(SeparatorInset::Text).children(rows), cx)
    }

    fn attachments_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let add = action_button("new-task-files", "Add files")
            .start_icon(button_icon("paperclip"))
            .track_focus(&self.stops.files)
            .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx)));
        let help = v_flex()
            .child(foot_text(ATTACH_HELP))
            .children(self.refused.iter().map(|reason| error_line(reason.clone())));
        SettingsRow::new("new-task-attachments", "Attachments")
            .help_child(help)
            .control(add)
            .into_any_element()
    }

    fn file_row(&self, file: &PickedFile, cx: &mut Context<Self>) -> AnyElement {
        let id = file.id;
        let name = Label::new(file.name.clone())
            .size(LabelSize::Small)
            .buffer_font(cx)
            .truncate();
        let remove = IconButton::new(("remove-file", id), IconName::Close)
            .icon_size(IconSize::XSmall)
            .tooltip(Tooltip::text(format!("Remove {}", file.name)))
            .on_click(cx.listener(move |this, _, _, cx| this.remove_file(id, cx)));
        h_flex()
            .w_full()
            .h(rems_from_px(32.0_f32))
            .gap(rems_from_px(8.0_f32))
            .pl(rems_from_px(12.0_f32))
            .pr(rems_from_px(8.0_f32))
            .child(
                Icon::new(IconName::File)
                    .size(IconSize::Custom(rems_from_px(13.0_f32)))
                    .color(Color::Muted),
            )
            .child(div().flex_1().min_w_0().overflow_hidden().child(name))
            .child(foot_text(format_bytes(file.size)))
            .child(remove)
            .into_any_element()
    }

    pub(super) fn workspace_card(&self, project: &str, cx: &mut Context<Self>) -> AnyElement {
        let avatar = SettingsTile::avatar(project, TileSize::Inline).into_any_element();
        let text = select_text(project.to_string(), Color::Default, None);
        let select = self.select("new-task-project", Pick::Project, avatar, text, false, cx);
        let mut rows = vec![
            SettingsRow::new("new-task-project-row", "Project")
                .control(select)
                .into_any_element(),
            SettingsRow::new("new-task-worktree-row", "Create a worktree")
                .help(worktree_help(self.worktree))
                .control(self.worktree_switch(cx))
                .into_any_element(),
        ];
        if self.worktree {
            rows.push(self.base_row(project, cx));
            rows.push(self.branch_row(cx));
        }
        SettingsCard::new(SeparatorInset::Text)
            .children(rows)
            .into_any_element()
    }

    /// The switch in a focus stop of its own: Space or Enter flips it, as a click does.
    fn worktree_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let switch = Switch::new("new-task-worktree-switch", self.worktree.into())
            .on_click(cx.listener(|this, _: &ToggleState, _, cx| this.toggle_worktree(cx)));
        div()
            .id("new-task-worktree")
            .flex_none()
            .track_focus(&self.stops.worktree)
            .tooltip(Tooltip::text(worktree_tip(self.worktree)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    this.toggle_worktree(cx);
                }
            }))
            .child(switch)
            .into_any_element()
    }

    fn toggle_worktree(&mut self, cx: &mut Context<Self>) {
        self.worktree = !self.worktree;
        cx.notify();
    }

    /// "Branch from": off, with a tooltip, until the project's branches are read.
    fn base_row(&self, project: &str, cx: &mut Context<Self>) -> AnyElement {
        let loading = self.branches.is_none();
        let note = shows_default(self.branches.as_ref(), &self.base).then(|| "· default".into());
        let text = select_text(self.base_name(), Color::Default, note);
        let lead = lead_icon("git-branch");
        let select = self.select("new-task-branch", Pick::Branch, lead, text, loading, cx);
        let tip = format!("Reading the branches of {project}\u{2026}");
        let control = div()
            .id("new-task-branch-tip")
            .when(loading, |wrap| wrap.tooltip(Tooltip::text(tip)))
            .child(select);
        let row = SettingsRow::new("new-task-branch-row", "Branch from").control(control);
        if loading {
            row.help("Reading the branches\u{2026}").into_any_element()
        } else {
            row.into_any_element()
        }
    }

    /// "Branch": a warning dot and a reset button before the input once the user typed a name.
    fn branch_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = self.title.read(cx).text(cx);
        let follows = prefill_branch(self.branch_pattern().as_deref(), &title).is_some();
        let mut row = SettingsRow::new("new-task-branch-name", "Branch")
            .help(branch_help(self.branch_edited, follows));
        if self.branch_edited {
            let warning = cx.theme().status().warning;
            let dot = div()
                .id("new-task-branch-mark")
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .border_1()
                .border_color(warning)
                .bg(warning)
                .tooltip(Tooltip::text("Changed"));
            let reset = IconButton::new("new-task-branch-reset", IconName::RotateCcw)
                .size(ButtonSize::Compact)
                .icon_size(IconSize::XSmall)
                .track_focus(&self.stops.reset)
                .tooltip(Tooltip::text("Follow the title again"))
                .on_click(cx.listener(|this, _, _, cx| this.follow_title(cx)));
            row = row.control(dot).control(reset);
        }
        row.control(SettingsInput::new(&self.branch_name).width(INPUT_WIDTH))
            .into_any_element()
    }

    pub(super) fn agent_card(
        &self,
        error_row: Option<ErrorRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let limits = current(&self.dashboard.read(cx).data.limits, now_ms());
        let (icon, text, note) = if self.agent.is_empty() {
            ("terminal", "No agent, a plain terminal".to_string(), None)
        } else {
            let note = picker_note(&self.agent, &limits).map(|note| format!("· {note}"));
            ("bot", agent_label(Some(&self.agent)), note)
        };
        let text = select_text(text, Color::Default, note);
        let select = self.select(
            "new-task-agent",
            Pick::Agent,
            lead_icon(icon),
            text,
            false,
            cx,
        );
        let this = cx.weak_entity();
        let options = [
            (SharedString::from("Full flow"), IconName::ListTree),
            (SharedString::from("One agent"), IconName::Sparkle),
        ];
        let selected = usize::from(self.single_agent);
        let flow = Segmented::new(
            "new-task-flow",
            &self.stops.flow,
            options,
            selected,
            move |index, _, cx| {
                this.update(cx, |this, cx| {
                    this.single_agent = index == 1;
                    cx.notify();
                })
                .ok();
            },
        );
        SettingsCard::new(SeparatorInset::Text)
            .child(SettingsRow::new("new-task-agent-row", "Agent").control(select))
            .child(
                SettingsRow::new("new-task-flow-row", "Task flow")
                    .error((error_row == Some(ErrorRow::Flow)).then_some(CHOOSE_AGENT))
                    .control(flow),
            )
            .into_any_element()
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
