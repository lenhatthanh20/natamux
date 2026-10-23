//! The hover buttons of a project card's header (Start a task, Settings, Blur names, the menu)
//! and the commands of the project menu.

use gpui::{ClickEvent, ElementId};
use ui::{ButtonLike, Tooltip, prelude::*};

use super::sidebar::lucide;
use super::workspace_tree::{Snapshot, WorkspaceTree};
use crate::components::confirm::Confirm;
use crate::projects::project_settings_dialog;
use crate::sidebar::sidebar_text::{
    ProjectCommand, remove_project_description, remove_project_title, shown_name,
};
use crate::sidebar::tree_model::GroupInfo;
use crate::sidebar::workspace_groups::RepoRef;
use crate::workspaces::{new_workspace_dialog, remove_finished};

/// A small icon button with a Lucide icon, shown on hover of `group`.
pub(super) fn hover_button(
    id: impl Into<ElementId>,
    icon: &str,
    label: String,
    group: SharedString,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> ButtonLike {
    ButtonLike::new(id)
        .style(ButtonStyle::Subtle)
        .child(lucide(icon).color(Color::Muted))
        .tooltip(Tooltip::text(label))
        .visible_on_hover(group)
        .on_click(on_click)
}

impl WorkspaceTree {
    /// The hover buttons of a header: Start a task, Settings, Blur names / Show names, and the
    /// menu.
    pub(super) fn header_buttons(
        &self,
        repo: &RepoRef,
        info: &GroupInfo,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let blurred = snap.blurred.contains(&repo.id);
        let name = shown_name(&repo.name, blurred);
        let group_name = SharedString::from(format!("header-{}", repo.id));
        let (new_repo, board) = (repo.id.clone(), self.dashboard.clone());
        let start = (!info.gone).then(|| {
            hover_button(
                SharedString::from(format!("start-{}", repo.id)),
                "plus",
                format!("Start a task in {name}"),
                group_name.clone(),
                move |_, window, cx| {
                    new_workspace_dialog::open(&board, Some(new_repo.clone()), window, cx);
                },
            )
            .into_any_element()
        });
        let (settings_repo, board) = (repo.id.clone(), self.dashboard.clone());
        let settings = (!info.gone).then(|| {
            hover_button(
                SharedString::from(format!("settings-{}", repo.id)),
                "settings",
                format!("Settings of {name}"),
                group_name.clone(),
                move |_, window, cx| {
                    project_settings_dialog::open(&board, &settings_repo, window, cx);
                },
            )
            .into_any_element()
        });
        let (blur_repo, board) = (repo.id.clone(), self.dashboard.clone());
        let mut blur = ButtonLike::new(SharedString::from(format!("blur-{}", repo.id)))
            .style(ButtonStyle::Subtle)
            .toggle_state(blurred)
            .child(lucide(if blurred { "eye" } else { "eye-off" }).color(Color::Muted))
            .tooltip(Tooltip::text(if blurred {
                "Show names"
            } else {
                "Blur names"
            }))
            .on_click(move |_, _, cx| {
                board.update(cx, |board, cx| board.toggle_blurred(&blur_repo, cx));
            });
        // Always on screen while blurred, so the user sees why the names are gone.
        if !blurred {
            blur = blur.visible_on_hover(group_name.clone());
        }
        let more_repo = repo.id.clone();
        let more = IconButton::new(
            SharedString::from(format!("more-{}", repo.id)),
            IconName::Ellipsis,
        )
        .icon_size(IconSize::Small)
        .visible_on_hover(group_name)
        .tooltip(Tooltip::text(format!("Actions for {name}")))
        .on_click(cx.listener(move |tree, event: &ClickEvent, window, cx| {
            tree.open_project_menu(&more_repo, event.position(), window, cx);
        }));
        start
            .into_iter()
            .chain(settings)
            .chain([blur.into_any_element(), more.into_any_element()])
            .collect()
    }

    pub(super) fn run_project_command(
        &mut self,
        repo_id: &str,
        command: ProjectCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let repo_id = repo_id.to_owned();
        match command {
            ProjectCommand::StartTask => {
                new_workspace_dialog::open(&self.dashboard, Some(repo_id), window, cx);
            }
            ProjectCommand::Settings => {
                project_settings_dialog::open(&self.dashboard, &repo_id, window, cx);
            }
            ProjectCommand::ToggleBlur => self
                .dashboard
                .update(cx, |dashboard, cx| dashboard.toggle_blurred(&repo_id, cx)),
            ProjectCommand::RemoveFinished => {
                remove_finished::open(&self.dashboard, repo_id, window, cx);
            }
            ProjectCommand::RemoveProject => {
                let dashboard = self.dashboard.read(cx);
                let Some(repo) = dashboard.data.repos.iter().find(|repo| repo.id == repo_id) else {
                    return;
                };
                let (title, description) = (
                    remove_project_title(&repo.name),
                    remove_project_description(&repo.path),
                );
                let dashboard = self.dashboard.clone();
                Confirm::new(title, description, "Remove", move |_, cx| {
                    dashboard.update(cx, |dashboard, cx| {
                        dashboard.remove_repo(&repo_id, cx).detach()
                    });
                })
                .open(window, cx);
            }
        }
    }
}
