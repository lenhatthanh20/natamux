//! How the Start a task dialog looks (T-0292, option B "Inset cards sheet"): a sheet with three
//! cards, Task, Workspace and Agent, a footnote under each, the daemon's error as an alert at the
//! top, and a footer note that says why Start task is off or what it will make.

use gpui::{AnyElement, DismissEvent, ExternalPaths};
use ui::{Tooltip, prelude::*};

use super::super::new_workspace_rules::{blocked_reason, start_label};
use super::super::new_workspace_view_rules::{
    FootPart, FooterNote, agent_foot, base_name, footer_note, inline_error, stage_names, start_tip,
    workspace_foot,
};
use super::NewWorkspaceDialog;
use crate::components::button_style::button_icon;
use crate::components::chips::tip_with_keys;
use crate::components::colors::TileColor;
use crate::components::dialog::{DialogFrame, cancel_button, primary_button};
use crate::components::settings_card::{SettingsGroup, foot_text};
use crate::components::settings_tile::{AlertKind, SettingsAlert, SettingsTile, TileSize};

impl NewWorkspaceDialog {
    /// The Branch field follows the title again (its reset button).
    pub(super) fn follow_title(&mut self, cx: &mut Context<Self>) {
        self.branch_edited = false;
        cx.notify();
    }

    /// The shown project's name, for its select and the footnotes.
    pub(super) fn repo_name(&self, cx: &App) -> String {
        let repo = self.repo(cx);
        self.dashboard
            .read(cx)
            .data
            .repos
            .iter()
            .find(|each| each.id == repo)
            .map_or_else(|| repo.clone(), |each| each.name.clone())
    }

    /// The stages a full flow runs; none until the project's settings are read.
    fn stage_names(&self) -> Vec<String> {
        self.settings
            .as_ref()
            .map(|loaded| stage_names(&loaded.settings.stages))
            .unwrap_or_default()
    }

    /// The branch the worktree starts from, by name.
    pub(super) fn base_name(&self) -> String {
        base_name(self.branches.as_ref(), &self.base)
    }

    /// The note left of the footer buttons: an info icon and the reason, or the plan.
    fn footer_note_line(&self, note: FooterNote) -> AnyElement {
        let (blocked, text) = match note {
            _ if self.busy => (false, "Starting the task\u{2026}".to_string()),
            FooterNote::Blocked(reason) => (true, reason.to_string()),
            FooterNote::Plan(plan) => (false, plan),
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .gap(rems_from_px(6.0_f32))
            .when(blocked, |line| {
                line.child(
                    Icon::new(IconName::Info)
                        .size(IconSize::XSmall)
                        .color(Color::Placeholder),
                )
            })
            .child(
                div().flex_1().min_w_0().overflow_hidden().child(
                    Label::new(text)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                ),
            )
            .into_any_element()
    }

    /// Start task with its tooltip on a wrapper, so the tooltip also shows while it is off.
    fn start_button(&self, reason: Option<&'static str>, cx: &mut Context<Self>) -> AnyElement {
        let off = reason.is_some() || self.busy;
        let submit = cx.listener(|this, _, window, cx| this.start_checked(window, cx));
        let button = primary_button(start_label(self.busy), &self.stops.submit, off, submit);
        let tip = start_tip(reason, self.busy, !self.worktree);
        div()
            .id("new-task-start-tip")
            .map(|wrap| {
                if off {
                    wrap.tooltip(Tooltip::text(tip))
                } else {
                    wrap.tooltip(tip_with_keys(tip, "mod-enter"))
                }
            })
            .child(button)
            .into_any_element()
    }

    /// The three groups, dimmed and closed to the pointer while the task starts.
    fn render_groups(&self, cx: &mut Context<Self>) -> AnyElement {
        let form = self.form(cx);
        let choice = self.choice();
        let error_row = inline_error(blocked_reason(&form, &choice));
        let stages = self.stage_names();
        let project = self.repo_name(cx);
        let task = SettingsGroup::new("Task")
            .first(self.error.is_none())
            .child(self.task_card(&choice, error_row, cx))
            .footnote(choice.context_hint(&self.skill));
        let workspace = SettingsGroup::new("Workspace")
            .child(self.workspace_card(&project, cx))
            .footnote_child(foot_line(
                workspace_foot(self.worktree, &form.branch, &self.base_name()),
                cx,
            ));
        let mut agent = SettingsGroup::new("Agent")
            .child(self.agent_card(error_row, cx))
            .footnote(agent_foot(self.single_agent, stages.len(), &project));
        if !self.single_agent && !stages.is_empty() {
            agent = agent.footnote_child(stage_chips(stages, cx));
        }
        div()
            .relative()
            .w_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if !this.busy {
                    this.add_paths(paths.paths().to_vec(), cx)
                }
            }))
            .child(
                v_flex()
                    .w_full()
                    .when(self.busy, |groups| groups.opacity(0.6))
                    .child(task)
                    .child(workspace)
                    .child(agent),
            )
            .when(self.busy, |body| {
                body.child(div().absolute().inset_0().occlude())
            })
            .into_any_element()
    }
}

/// A footnote line with names in the mono font; it wraps between its parts.
fn foot_line(parts: Vec<FootPart>, cx: &App) -> impl IntoElement {
    h_flex().flex_wrap().children(parts.into_iter().map(|part| {
        match part {
            FootPart::Text(text) => foot_text(text).into_any_element(),
            FootPart::Mono(name) => Label::new(name)
                .size(LabelSize::Custom(rems_from_px(11.0_f32)))
                .buffer_font(cx)
                .into_any_element(),
        }
    }))
}

/// The stages of the full flow as chips joined by chevrons.
fn stage_chips(stages: Vec<String>, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let mut row = h_flex()
        .flex_wrap()
        .gap(rems_from_px(4.0_f32))
        .mt(rems_from_px(6.0_f32));
    for (index, name) in stages.into_iter().enumerate() {
        if index > 0 {
            row = row.child(
                Icon::new(IconName::ChevronRight)
                    .size(IconSize::Custom(rems_from_px(10.0_f32)))
                    .color(Color::Placeholder),
            );
        }
        row = row.child(
            h_flex()
                .flex_none()
                .h(rems_from_px(18.0_f32))
                .px(rems_from_px(6.0_f32))
                .rounded(rems_from_px(4.0_f32))
                .bg(colors.element_background)
                .border_1()
                .border_color(colors.border_variant)
                .child(
                    Label::new(name)
                        .size(LabelSize::Custom(rems_from_px(11.0_f32)))
                        .color(Color::Muted),
                ),
        );
    }
    row
}

impl Render for NewWorkspaceDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_branch(window, cx);
        let form = self.form(cx);
        let choice = self.choice();
        let reason = blocked_reason(&form, &choice);
        let note = footer_note(&form, &choice, &self.base_name(), self.stage_names().len());
        let close = cx.listener(|this, _, _, cx| {
            if !this.busy {
                cx.emit(DismissEvent)
            }
        });
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cancel_button("Cancel", &self.stops.cancel, cancel).disabled(self.busy);
        let start = self.start_button(reason, cx);
        let tile =
            SettingsTile::icon(button_icon("play"), TileColor::Blue, TileSize::Row).glyph_px(12.);
        let alert = self.error.clone().map(|error| {
            SettingsAlert::new(AlertKind::Error, "Could not start the task")
                .muted_text(format!("The daemon said: {error}"))
        });
        let groups = self.render_groups(cx);
        let frame = DialogFrame::new("Start a task", close)
            .sheet()
            .body_scroll(&self.scroll)
            .lead(tile)
            .footer_start(self.footer_note_line(note))
            .children(alert)
            .child(groups)
            .footer(cancel)
            .footer(start);
        div().track_focus(&self.focus).child(frame)
    }
}
