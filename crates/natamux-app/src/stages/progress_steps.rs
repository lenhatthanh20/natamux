//! The group and branch rows of the Progress tab: a group's name and note with its cards side by
//! side, and a branch with one lane per path (`BranchLane.tsx`); a path not taken folds to one row.

use gpui::{AnyElement, FontWeight};
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{BranchPath, StageBranch, StageGroup, Step};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::{Ctx, heading};
use crate::stages::stage_card::text_chip;
use crate::stages::stage_view_rules::{branch_status, path_label, taken_path};
use crate::tasks::details_commands::PanelCommand;

impl Ctx<'_> {
    pub(super) fn group(&self, group: &StageGroup, cx: &App) -> AnyElement {
        let name = group.name.clone().unwrap_or_else(|| {
            group
                .parallel
                .iter()
                .map(|stage| stage.id.as_str())
                .collect::<Vec<_>>()
                .join(" and ")
        });
        let steps = &self.input.steps;
        let index = steps
            .iter()
            .position(|step| step.id() == group.id)
            .unwrap_or(0);
        let note = match steps.get(index + 1) {
            None => "The task is done when both are finished".to_owned(),
            Some(Step::Branch(_)) => "A path is chosen when both are finished".to_owned(),
            Some(next) => {
                let first = next
                    .stages()
                    .first()
                    .map(|stage| stage.id.clone())
                    .unwrap_or_default();
                let name = self
                    .by_id
                    .get(first.as_str())
                    .map_or(first.clone(), |entry| entry.name.clone());
                format!("{name} starts when both are finished")
            }
        };
        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .child(heading(name))
                    .child(
                        Label::new("side by side")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(Label::new(note).size(LabelSize::XSmall).color(Color::Muted)),
            )
            .children(
                group
                    .parallel
                    .iter()
                    .filter_map(|stage| self.card(&stage.id, cx)),
            )
            .into_any_element()
    }

    pub(super) fn branch(&self, branch: &StageBranch, cx: &App) -> AnyElement {
        let lookup = |id: &str| self.by_id.get(id).map(|entry| entry.status);
        let taken = taken_path(branch, &lookup);
        let reached = branch_status(branch, &lookup) != StageStatus::Upcoming;
        let name = branch.name.clone().unwrap_or_else(|| {
            branch
                .paths
                .iter()
                .map(path_label)
                .collect::<Vec<_>>()
                .join(" or ")
        });
        let because = taken.and_then(|path| {
            path.stages
                .first()
                .and_then(|stage| self.by_id.get(stage.id.as_str()))
                .and_then(|entry| entry.because.clone())
        });
        let line = match (taken, reached) {
            (Some(path), _) => match because {
                Some(because) => format!("Took {}: {because}", path_label(path)),
                None => format!("Took {}", path_label(path)),
            },
            (None, true) => "No path was taken".to_owned(),
            (None, false) => "Picks a path when it gets here".to_owned(),
        };
        let lanes = branch.paths.iter().map(|path| {
            let state = match taken {
                Some(taken) if taken.id == path.id => Some(true),
                Some(_) => Some(false),
                None if reached => Some(false),
                None => None,
            };
            self.lane(branch, path, state, cx)
        });
        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .child(heading(name))
                    .child(Label::new(line).size(LabelSize::XSmall).color(Color::Muted)),
            )
            .children(lanes)
            .into_any_element()
    }

    /// One path: folded to one row when not taken, else its header and its cards.
    fn lane(
        &self,
        branch: &StageBranch,
        path: &BranchPath,
        taken: Option<bool>,
        cx: &App,
    ) -> AnyElement {
        let key = format!("{}/{}", branch.id, path.id);
        if taken == Some(false) {
            return self.not_taken(&key, path, cx);
        }
        let colors = cx.theme().colors();
        let taken = taken == Some(true);
        let title_color = if taken { Color::Default } else { Color::Muted };
        v_flex()
            .gap_2()
            .pl_2()
            .border_l_2()
            .border_color(if taken {
                colors.border_focused
            } else {
                colors.border_variant
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(caps(path_label(path), title_color))
                    .child(small(describe_condition(path)))
                    .when(taken, |this| this.child(small("Taken".into()))),
            )
            .children(
                path.stages
                    .iter()
                    .filter_map(|stage| self.card(&stage.id, cx)),
            )
            .into_any_element()
    }

    /// A path the task did not take: one row that opens to its Not taken stages.
    fn not_taken(&self, key: &str, path: &BranchPath, cx: &App) -> AnyElement {
        let opened = self.input.open_lanes.contains(key);
        let on = self.on.clone();
        let count = path.stages.len();
        let stages = if count == 1 {
            "1 stage".to_owned()
        } else {
            format!("{count} stages")
        };
        let fold = PanelCommand::FoldLane(key.to_owned());
        let chevron = if opened {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let row = v_flex()
            .id(SharedString::from(format!("lane-{key}")))
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().colors().ghost_element_hover))
            .on_click(move |_, window, cx| on(fold.clone(), window, cx))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(chevron)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(caps(path_label(path), Color::Muted)),
            )
            .child(small(describe_condition(path)))
            .child(
                h_flex()
                    .gap_2()
                    .child(text_chip("Not taken", TaskState::Draft, cx))
                    .child(small(stages)),
            );
        v_flex()
            .pl_2()
            .border_l_1()
            .border_color(cx.theme().colors().border_variant)
            .child(row)
            .when(opened, |this| {
                this.children(
                    path.stages
                        .iter()
                        .filter_map(|stage| self.card(&stage.id, cx)),
                )
            })
            .into_any_element()
    }
}

fn caps(name: &str, color: Color) -> Label {
    Label::new(name.to_uppercase())
        .size(LabelSize::XSmall)
        .weight(FontWeight::SEMIBOLD)
        .color(color)
}

fn small(text: String) -> Label {
    Label::new(text).size(LabelSize::XSmall).color(Color::Muted)
}
