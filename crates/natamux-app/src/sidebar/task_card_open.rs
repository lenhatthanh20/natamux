//! The open card (Option F): title up to two lines, chips (project, id, stage, pull request,
//! subtasks), the waiting box, the branch line, then the terminal rows and the subtask cards
//! under thin dividers. Also the pieces a closed card shares: the waiting box, the project and
//! the blur check.

use gpui::{AnyElement, ClickEvent, Hsla};
use natamux_proto::tasks::task::Task;
use ui::{Tooltip, prelude::*};

use super::blur_bar::{BlurBar, private_text};
use super::sidebar::{lucide, mono, small};
use super::task_card::{CONTENT_INSET, inset};
use super::workspace_tree::{Snapshot, WorkspaceTree};
use crate::components::button_style::action_button;
use crate::components::colors::{project_color, state_colors};
use crate::sidebar::card_rules::{WaitButton, Waiting, pr_chip, stage_chip};
use crate::sidebar::project_color::project_color as project_color_index;
use crate::sidebar::shell_rows::ShellRow;
use crate::sidebar::tree_entries::EntryAction;
use crate::sidebar::tree_model::TreeModel;
use crate::sidebar::workspace_groups::DEFAULT_REPO_ID;
use crate::tasks::task_table_cells::diff_label;

impl WorkspaceTree {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn open_card(
        &self,
        task: &Task,
        rows: &[ShellRow],
        children: &[Task],
        model: &TreeModel,
        snap: &Snapshot,
        pinned_row: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blurred = is_blurred(task, snap);
        let divider = cx.theme().colors().border_variant;
        let head = self.card_head(task, model, snap, true, cx);
        let chips = self.meta_chips(task, snap, pinned_row, blurred, cx);
        let waiting = self.waiting_box(task, snap, blurred, true, cx);
        let branch = self.branch_line(task, snap, blurred, cx);
        let terminals = (!rows.is_empty()).then(|| {
            let list: Vec<AnyElement> = rows
                .iter()
                .map(|row| self.terminal_row(&task.id, row, snap, cx))
                .collect();
            // A click on a terminal row must not also open the task behind it. The rows are the
            // task's children, so their dot column starts at the title edge: the inset less the
            // row's border and padding (1 + 8 px). The divider stays full width (T-0255).
            div()
                .id(SharedString::from(format!("terminals-{}", task.id)))
                .w_full()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(divider)
                .pt_1()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .ml(rems_from_px(CONTENT_INSET - 9.0))
                        .children(list),
                )
                .on_click(|_, _, cx| cx.stop_propagation())
                .into_any_element()
        });
        let subtasks = (!children.is_empty()).then(|| {
            let cards: Vec<AnyElement> = children
                .iter()
                .map(|child| self.closed_card(child, model, snap, false, cx))
                .collect();
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(rems_from_px(6.0_f32))
                .border_t_1()
                .border_color(divider)
                .pt(rems_from_px(7.0_f32))
                .children(cards)
                .into_any_element()
        });
        self.card_frame(task, snap, [9.0, 9.0, 10.0, 10.0], 7.0, true, cx)
            .child(head)
            .child(chips)
            .children(waiting)
            .children(branch)
            .children(terminals)
            .children(subtasks)
            .into_any_element()
    }

    /// Project (pinned), id, stage, pull request and subtask count as small chips.
    fn meta_chips(
        &self,
        task: &Task,
        snap: &Snapshot,
        pinned_row: bool,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips: Vec<AnyElement> = Vec::new();
        if pinned_row {
            let (name, color) = self.project_of(task, cx);
            let text = private_text(
                blurred,
                || BlurBar::new(name.clone(), 12.0),
                || {
                    Label::new(name.clone())
                        .size(LabelSize::Custom(rems_from_px(12_f32)))
                        .color(Color::Custom(color))
                        .single_line()
                        .into_any_element()
                },
            );
            chips.push(chip(text, cx));
        }
        chips.push(chip(mono(task.id.clone(), cx).into_any_element(), cx));
        if let Some(stage) = stage_chip(task) {
            let name = private_text(
                blurred,
                || BlurBar::new(stage.stage.clone(), 12.0),
                || {
                    small(stage.stage.clone())
                        .color(Color::Default)
                        .into_any_element()
                },
            );
            let body = h_flex()
                .gap_1()
                .children(stage.step.map(small))
                .child(name)
                .into_any_element();
            chips.push(chip(body, cx));
        }
        if let Some(pr) = pr_chip(task) {
            let color = pr.tone.map_or(Color::Muted, |tone| {
                Color::Custom(state_colors(tone, cx).fg)
            });
            let text = Label::new(pr.text)
                .size(LabelSize::Custom(rems_from_px(12_f32)))
                .color(color)
                .single_line();
            chips.push(chip(text.into_any_element(), cx));
        }
        if let Some(count) = snap.subtasks.get(&task.id) {
            chips.push(chip(
                small(format!("{count} subtasks")).into_any_element(),
                cx,
            ));
        }
        inset(
            h_flex()
                .debug_selector(|| format!("chips-{}", task.id))
                .w_full()
                .flex_wrap()
                .gap(rems_from_px(5.0_f32))
                .children(chips),
        )
        .into_any_element()
    }

    /// Branch name, diff, and a copy button on hover.
    fn branch_line(
        &self,
        task: &Task,
        snap: &Snapshot,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let branch = task.branch.clone()?;
        let group = SharedString::from(format!("row-{}", task.id));
        let name = private_text(
            blurred,
            || BlurBar::new(branch.clone(), 11.0).mono(),
            || mono(branch.clone(), cx).truncate().into_any_element(),
        );
        let copy = (!blurred).then(|| {
            let text = branch.clone();
            IconButton::new(
                SharedString::from(format!("copy-{}", task.id)),
                IconName::Copy,
            )
            .icon_size(IconSize::Small)
            .visible_on_hover(group)
            .tooltip(Tooltip::text("Copy branch name"))
            .on_click(cx.listener(move |tree, _: &ClickEvent, _window, cx| {
                tree.copy(text.clone(), "branch name", cx);
                cx.stop_propagation();
            }))
        });
        Some(
            inset(
                h_flex()
                    .debug_selector(|| format!("branch-{}", task.id))
                    .w_full()
                    .gap(rems_from_px(5.0_f32))
                    .child(lucide("git-branch").color(Color::Muted))
                    .child(div().min_w_0().flex_1().child(name))
                    .children(diff_label(snap.diff_stats.get(&task.id)))
                    .children(copy),
            )
            .into_any_element(),
        )
    }

    /// The tinted strip (closed) or box (open) of a task that needs the user.
    pub(super) fn waiting_box(
        &self,
        task: &Task,
        snap: &Snapshot,
        blurred: bool,
        open: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let waiting = snap.waiting.get(&task.id)?;
        let tint = state_colors(task.state, cx).tinted();
        let text = private_text(
            blurred,
            || BlurBar::new(waiting.text.clone(), 12.0),
            || {
                let label =
                    Label::new(waiting.text.clone()).size(LabelSize::Custom(rems_from_px(12_f32)));
                if open {
                    label.line_clamp(4).into_any_element()
                } else {
                    label.single_line().truncate().into_any_element()
                }
            },
        );
        Some(
            inset(
                h_flex()
                    .debug_selector(|| format!("waiting-{}", task.id))
                    .w_full()
                    .gap(rems_from_px(6.0_f32))
                    .px(rems_from_px(7.0_f32))
                    .rounded(px(5.))
                    .bg(tint.bg)
                    .when(!open, |strip| strip.h(rems_from_px(22.0_f32)))
                    .when(open, |strip| strip.py(rems_from_px(5.0_f32)).items_start())
                    .child(div().min_w_0().flex_1().child(text))
                    .child(self.wait_button(task, waiting, cx)),
            )
            .into_any_element(),
        )
    }

    fn wait_button(&self, task: &Task, waiting: &Waiting, cx: &mut Context<Self>) -> AnyElement {
        let id = task.id.clone();
        let kind = waiting.button;
        div()
            .flex_none()
            .child(
                action_button(SharedString::from(format!("wait-{id}")), kind.label())
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(move |tree, _: &ClickEvent, _window, cx| {
                        match kind {
                            WaitButton::OpenTask => {
                                tree.run_entry(EntryAction::OpenTask(id.clone()), cx)
                            }
                            WaitButton::Answer | WaitButton::OpenTerminal => tree.answer(&id, cx),
                        }
                        cx.stop_propagation();
                    })),
            )
            .into_any_element()
    }

    /// The name of the card's project and its colour, for a pinned card.
    pub(super) fn project_of(&self, task: &Task, cx: &App) -> (String, Hsla) {
        let repo = task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID);
        let name = self
            .dashboard
            .read(cx)
            .data
            .repos
            .iter()
            .find(|known| known.id == repo)
            .map_or_else(|| repo.to_owned(), |known| known.name.clone());
        (name, project_color(project_color_index(repo), cx))
    }
}

pub(super) fn is_blurred(task: &Task, snap: &Snapshot) -> bool {
    let repo = task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID);
    snap.blurred.iter().any(|id| id == repo)
}

/// A small rounded chip on the meta row: no background of its own, so the card's fill stays even.
fn chip(child: AnyElement, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .h(rems_from_px(18.0_f32))
        .px(rems_from_px(6.0_f32))
        .rounded(px(5.))
        .border_1()
        .border_color(colors.border_variant)
        .child(child)
        .into_any_element()
}

/// The title of an open card: 13.5 semibold, up to two lines, never truncated to one.
pub(super) fn open_title(task: &Task, blurred: bool) -> AnyElement {
    private_text(
        blurred,
        || BlurBar::new(task.title.clone(), 13.5).weight(gpui::FontWeight::SEMIBOLD),
        || {
            Label::new(task.title.clone())
                .size(LabelSize::Custom(rems_from_px(13.5_f32)))
                .weight(gpui::FontWeight::SEMIBOLD)
                .line_clamp(2)
                .into_any_element()
        },
    )
}
