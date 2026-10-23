//! The Details tab (`TaskDetail.tsx`): the title block with its chips, the quiet and waiting
//! banners, the action row, then the sections Notes, Workspace, Pull request, Usage, Subtasks,
//! Tries, Details and Activity (the last four are in `details_lists.rs`).

use gpui::{AnyElement, FontWeight};
use natamux_proto::tasks::task::Task;
use ui::{ButtonLike, TintColor, Tooltip, prelude::*};

use super::DetailsPanel;
use crate::app::clock::now_ms;
use crate::components::button_style::{label_color, tinted_style};
use crate::components::chips::{StateChip, label_chip, tip_with_keys};
use crate::components::colors::signal;
use crate::components::detail_section::{DetailSection, field};
use crate::components::icons::icon_path;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::details_rules::{
    NO_NOTES, NO_WORKSPACE, PORTS_HINT, agent_button, details_start_branch, ports_text,
    pull_request_label, pull_request_summary, quiet_banner, waits_for_text,
};
use crate::tasks::edit_rules::label_text;
use crate::tasks::table_rules::waits_for;
use crate::tasks::task_attention::is_quiet;
use crate::tasks::task_rules::{actions_for, agent_action};
use crate::workspaces::wrap_up_rules::is_finished_state;

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn send(
    on: &OnCommand,
    command: PanelCommand,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    let on = on.clone();
    move |_, window, cx| on(command.clone(), window, cx)
}

impl DetailsPanel {
    pub(super) fn fold(
        &self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> impl Fn(bool, &mut Window, &mut App) + 'static {
        let on = self.on_command(cx);
        move |open, window, cx| on(PanelCommand::FoldSection { id, open }, window, cx)
    }

    pub(super) fn open_of(&self, id: &str, default: bool, cx: &App) -> bool {
        self.dashboard.read(cx).view().section_open(id, default)
    }

    pub(super) fn render_details(
        &mut self,
        task: &Task,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let on = self.on_command(cx);
        let waiting = waits_for(task, &self.dashboard.read(cx).data.tasks);
        let mut parts = vec![
            self.title_block(task, &waiting, &on, cx).into_any_element(),
            self.notes(task, cx).into_any_element(),
            self.workspace(task, &on, cx).into_any_element(),
        ];
        if task.pr.is_some() {
            parts.push(self.pull_request(task, &on, cx).into_any_element());
        }
        parts.extend(self.lists(task, &on, window, cx));
        parts
    }

    fn title_block(
        &self,
        task: &Task,
        waiting: &[String],
        on: &OnCommand,
        cx: &App,
    ) -> impl IntoElement {
        let now = now_ms();
        let tone = signal(cx);
        let banner = |text: String| {
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(tone.tinted().border)
                .bg(tone.tinted().bg)
                .child(Label::new(text).size(LabelSize::Small))
        };
        let chips = h_flex()
            .flex_wrap()
            .gap_1p5()
            .child(StateChip::new(task.state))
            .when(task.priority != "normal", |this| {
                this.child(label_chip(capitalized(&task.priority)))
            })
            .children(
                task.labels
                    .iter()
                    .map(|label| label_chip(label_text(label))),
            );
        v_flex()
            .p_3()
            .gap_2()
            .child(
                Label::new(task.title.clone())
                    .size(LabelSize::Large)
                    .weight(FontWeight::SEMIBOLD),
            )
            .child(chips)
            .when(is_quiet(task, now), |this| {
                this.child(banner(quiet_banner(&task.updated_at, now, 0)))
            })
            .children(waits_for_text(waiting).map(banner))
            .child(self.action_row(task, waiting, on))
    }

    /// The state actions without the one that asks first, then Start or Restart agent.
    fn action_row(&self, task: &Task, waiting: &[String], on: &OnCommand) -> impl IntoElement {
        let busy = self.shown.as_ref().is_some_and(|shown| shown.busy);
        let actions: Vec<_> = actions_for(task.state)
            .into_iter()
            .filter(|action| !action.confirm)
            .collect();
        let buttons =
            actions.into_iter().enumerate().map(|(index, action)| {
                let style = if index == 0 {
                    tinted_style(TintColor::Accent, busy)
                } else {
                    ButtonStyle::Outlined
                };
                let label = action.label;
                let mut button =
                    ButtonLike::new(SharedString::from(format!("action-{}", action.event_type)))
                        .style(style)
                        .disabled(busy)
                        .on_click(send(on, PanelCommand::TaskEvent(action.event_type)))
                        .child(
                            h_flex()
                                .gap_2()
                                .px_1()
                                .child(
                                    Label::new(label)
                                        .size(LabelSize::Small)
                                        .color(label_color(busy)),
                                )
                                .children(action.hotkey.map(|key| {
                                    crate::components::chips::Kbd::new(&key.to_string())
                                })),
                        );
                if let Some(key) = action.hotkey {
                    button = button.tooltip(tip_with_keys(label, &key.to_string()));
                }
                button.into_any_element()
            });
        let agent = agent_action(task.state, task.agent.as_deref()).map(|action| {
            let shown = agent_button(action, waiting, busy);
            ButtonLike::new("details-agent")
                .style(ButtonStyle::Outlined)
                .disabled(shown.disabled)
                .tooltip(Tooltip::text(shown.tip))
                .on_click(send(on, PanelCommand::StartAgent))
                .child(
                    h_flex()
                        .gap_1p5()
                        .px_1()
                        .child(Icon::from_path(icon_path(shown.icon)).size(IconSize::Small))
                        .child(Label::new(shown.label).size(LabelSize::Small)),
                )
                .into_any_element()
        });
        h_flex()
            .flex_wrap()
            .gap_1p5()
            .children(buttons)
            .children(agent)
    }

    fn notes(&self, task: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, color) = if task.description.is_empty() {
            (NO_NOTES.to_owned(), Color::Muted)
        } else {
            (task.description.clone(), Color::Default)
        };
        DetailSection::new(
            "notes",
            "Notes",
            self.open_of("notes", true, cx),
            self.fold("notes", cx),
        )
        .child(Label::new(text).size(LabelSize::Small).color(color))
    }

    fn workspace(&self, task: &Task, on: &OnCommand, cx: &mut Context<Self>) -> impl IntoElement {
        let section = DetailSection::new(
            "workspace",
            "Workspace",
            self.open_of("workspace", true, cx),
            self.fold("workspace", cx),
        );
        let busy = self.shown.as_ref().is_some_and(|shown| shown.busy);
        let Some(folder) = task.workspace.clone() else {
            if is_finished_state(task.state) {
                return section.child(
                    Label::new(NO_WORKSPACE)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                );
            }
            return section.child(self.no_workspace(task, on, busy, cx));
        };
        let branch = task.branch.clone().unwrap_or_default();
        // A long branch or folder wraps, so its copy button stays in the panel.
        let mono = |text: String| {
            div()
                .flex_1()
                .min_w_0()
                .child(Label::new(text).size(LabelSize::Small).buffer_font(cx))
        };
        let copy = |id: &'static str, text: String, what: &'static str| {
            div().flex_none().child(
                IconButton::new(id, IconName::Copy)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(if what == "the branch name" {
                        "Copy the branch name"
                    } else {
                        "Copy the folder path"
                    }))
                    .on_click(send(on, PanelCommand::CopyText { text, what })),
            )
        };
        section
            .child(field(
                "Branch",
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_1()
                    .child(mono(branch.clone()))
                    .child(copy("copy-branch", branch, "the branch name")),
            ))
            .child(field(
                "Folder",
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_1()
                    .child(mono(folder.clone()))
                    .child(copy("copy-folder", folder, "the folder path")),
            ))
            .child(field(
                "Dev ports",
                h_flex()
                    .child(
                        Label::new(ports_text(&task.id))
                            .size(LabelSize::Small)
                            .buffer_font(cx),
                    )
                    .child(
                        Label::new(PORTS_HINT)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            ))
            .child(workspace_buttons(on, busy))
    }

    /// "Start from branch" and Create workspace, for a task with no workspace.
    fn no_workspace(&self, task: &Task, on: &OnCommand, busy: bool, cx: &App) -> AnyElement {
        let shown = self.shown.as_ref();
        let list = shown.and_then(|shown| shown.branches.as_ref());
        let parent = task
            .parent_id
            .as_deref()
            .and_then(|id| self.dashboard.read(cx).data.task(id))
            .and_then(|parent| parent.branch.clone());
        let start = details_start_branch(parent.as_deref(), list);
        let picked = shown.and_then(|shown| shown.base.clone()).or(start);
        let many = list.is_some_and(|list| list.branches.len() > 1);
        let picker = many.then(|| {
            let options: Vec<String> = list.map(|list| list.branches.clone()).unwrap_or_default();
            let on = on.clone();
            let shown_text = picked.clone().unwrap_or_else(|| "HEAD".into());
            Button::new("details-base", shown_text)
                .style(ButtonStyle::Outlined)
                .label_size(LabelSize::Small)
                .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::XSmall))
                .tooltip(Tooltip::text("Start from branch"))
                .on_click(move |event, window, cx| {
                    let on = on.clone();
                    let options = options.clone();
                    let menu = ui::ContextMenu::build(window, cx, move |menu, _, _| {
                        options.iter().fold(menu, |menu, branch| {
                            let (on, branch) = (on.clone(), branch.clone());
                            menu.entry(branch.clone(), None, move |window, cx| {
                                on(PanelCommand::PickBase(branch.clone()), window, cx)
                            })
                        })
                    });
                    crate::components::menu::MenuLayer::open(menu, event.position(), window, cx);
                })
        });
        v_flex()
            .gap_2()
            .children(picker)
            .child(
                Button::new("details-create-workspace", "Create workspace")
                    .style(ButtonStyle::Outlined)
                    .label_size(LabelSize::Small)
                    .disabled(busy)
                    .on_click(send(on, PanelCommand::CreateWorkspace)),
            )
            .into_any_element()
    }

    fn pull_request(
        &self,
        task: &Task,
        on: &OnCommand,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let url = task.pr.clone().unwrap_or_default();
        let link = url.clone();
        let busy = self.shown.as_ref().is_some_and(|shown| shown.busy);
        DetailSection::new(
            "pull-request",
            "Pull request",
            self.open_of("pull-request", true, cx),
            self.fold("pull-request", cx),
        )
        .child(
            Button::new("details-pr-link", pull_request_label(&url).to_owned())
                .label_size(LabelSize::Small)
                .end_icon(Icon::from_path(icon_path("external-link")).size(IconSize::XSmall))
                .on_click(move |_, _, cx| cx.open_url(&link)),
        )
        .children(pull_request_summary(task).map(|summary| {
            Label::new(summary)
                .size(LabelSize::Small)
                .color(Color::Muted)
        }))
        .child(
            Button::new("details-check-pr", "Check now")
                .label_size(LabelSize::Small)
                .disabled(busy)
                .on_click(send(on, PanelCommand::CheckPullRequest)),
        )
    }
}

/// View changes and Remove workspace, under the workspace's fields.
fn workspace_buttons(on: &OnCommand, busy: bool) -> impl IntoElement + use<> {
    h_flex()
        .flex_wrap()
        .gap_2()
        .child(
            Button::new("details-view-changes", "View changes")
                .label_size(LabelSize::Small)
                .style(ButtonStyle::Outlined)
                .start_icon(Icon::from_path(icon_path("file-diff")).size(IconSize::Small))
                .on_click(send(on, PanelCommand::ViewChanges)),
        )
        .child(
            Button::new("details-remove-workspace", "Remove workspace")
                .label_size(LabelSize::Small)
                .start_icon(Icon::from_path(icon_path("folder-minus")).size(IconSize::Small))
                .disabled(busy)
                .on_click(send(on, PanelCommand::RemoveWorkspace)),
        )
}
