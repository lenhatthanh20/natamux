//! The path cards of the branch dialog: name, condition ("If [type] [holds]" and its fields),
//! the Otherwise toggle on the last path, and Remove path. They edit the dialog's own copy of the
//! draft (`local`), so nothing reaches the real draft before the dialog is saved.

use gpui::{AnyElement, ClickEvent, Entity, FontWeight};
use natamux_proto::stages::schema::{BranchPath, Condition, StageBranch};
use ui::prelude::*;

use super::branch_dialog_fields::{BranchFields, PathFields};
use super::editor_branches::{
    ConditionType, condition_problems_by_field, remove_path, retype_condition, set_otherwise,
    set_path_condition,
};
use super::kind_icon::remove_icon;
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{
    Choice, LiveField, change, error_line, help_line, labelled, open_choices, select,
};
use super::stage_options::checkbox_row;
use crate::components::confirm::Confirm;
use crate::components::form::input_box;

/// The cards of every path of `branch`.
pub(super) fn path_cards(
    branch: &StageBranch,
    fields: &BranchFields,
    local: &Entity<PipelineEditor>,
    cx: &App,
) -> Vec<AnyElement> {
    branch
        .paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let card = PathCard {
                branch,
                path,
                index,
                is_last: index + 1 == branch.paths.len(),
            };
            path_card(&card, fields, local, cx)
        })
        .collect()
}

struct PathCard<'a> {
    branch: &'a StageBranch,
    path: &'a BranchPath,
    index: usize,
    is_last: bool,
}

fn path_card(
    card: &PathCard,
    fields: &BranchFields,
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> AnyElement {
    let PathCard {
        branch,
        path,
        index,
        is_last,
    } = card;
    let label = format!("Path {}", index + 1);
    let Some(own) = fields.path(&path.id) else {
        return div().into_any_element();
    };
    let colors = cx.theme().colors();
    let otherwise = path.when.is_none();
    let condition = match &path.when {
        None => help_line("Taken when no condition above holds.").into_any_element(),
        Some(when) => condition_fields(card, own, when, pipeline, cx),
    };
    let toggle = is_last.then(|| {
        let (pipeline, branch_id, path_id) = (pipeline.clone(), branch.id.clone(), path.id.clone());
        checkbox_row(
            format!("otherwise-{}", path.id),
            "Make this the Otherwise path",
            otherwise,
            move |on, _, cx| {
                change(&pipeline, cx, |steps| {
                    set_otherwise(steps, &branch_id, &path_id, on)
                })
            },
            cx,
        )
    });
    v_flex()
        .gap_3()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.elevated_surface_background)
        .child(
            h_flex()
                .gap_2()
                .child(Label::new(label.clone()).weight(FontWeight::SEMIBOLD))
                .when(otherwise, |this| {
                    this.child(
                        div()
                            .px_1p5()
                            .rounded_md()
                            .bg(colors.element_background)
                            .child(
                                Label::new("Otherwise")
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    )
                })
                .child(div().flex_1())
                .child(remove_button(card, &label, pipeline)),
        )
        .child(labelled(
            "Path name",
            None,
            None,
            input_box(&own.name.editor, cx),
        ))
        .child(condition)
        .children(toggle)
        .into_any_element()
}

fn remove_button(
    card: &PathCard,
    label: &str,
    pipeline: &Entity<PipelineEditor>,
) -> impl IntoElement {
    let (pipeline, branch_id, path_id) = (
        pipeline.clone(),
        card.branch.id.clone(),
        card.path.id.clone(),
    );
    let has_stages = !card.path.stages.is_empty();
    let (title, id) = (
        format!("Remove {label}?"),
        format!("remove-{}", card.path.id),
    );
    Button::new(SharedString::from(id), "Remove path")
        .style(ButtonStyle::Subtle)
        .start_icon(remove_icon().size(IconSize::Small))
        .disabled(card.branch.paths.len() < 2)
        .on_click(move |_, window, cx| {
            let (pipeline, branch_id, path_id) =
                (pipeline.clone(), branch_id.clone(), path_id.clone());
            let remove = move |cx: &mut App| {
                change(&pipeline, cx, |steps| {
                    remove_path(steps, &branch_id, &path_id)
                })
            };
            if has_stages {
                Confirm::new(
                    title.clone(),
                    "Its stages and their settings are lost.",
                    "Remove path",
                    move |_, cx| remove(cx),
                )
                .open_over(window, cx);
            } else {
                remove(cx);
            }
        })
}

/// "If", the condition type and whether it holds.
fn condition_selects(
    card: &PathCard,
    when: &Condition,
    pipeline: &Entity<PipelineEditor>,
) -> impl IntoElement {
    let not = card.path.not == Some(true);
    let (branch_id, path_id) = (card.branch.id.clone(), card.path.id.clone());
    let set = {
        let (pipeline, branch_id, path_id) = (pipeline.clone(), branch_id, path_id);
        move |cx: &mut App, next: Condition, not: bool| {
            change(&pipeline, cx, |steps| {
                set_path_condition(steps, &branch_id, &path_id, &next, not)
            })
        }
    };
    let on_command = matches!(when, Condition::Command { .. });
    let kind_set = set.clone();
    let type_when = when.clone();
    let kind_button = select(
        SharedString::from(format!("condition-type-{}", card.path.id)),
        if on_command {
            "Command succeeds"
        } else {
            "File has text"
        },
    )
    .on_click(move |event: &ClickEvent, window, cx| {
        let choices = [
            (ConditionType::Output, "File has text"),
            (ConditionType::Command, "Command succeeds"),
        ]
        .map(|(kind, label)| {
            let (set, when) = (kind_set.clone(), type_when.clone());
            let checked = (kind == ConditionType::Command) == on_command;
            Choice::new(label, Some(checked), move |_, cx| {
                set(cx, retype_condition(Some(&when), kind), not)
            })
        });
        open_choices(choices.into(), event.position(), window, cx);
    });
    let holds_when = when.clone();
    let holds_button = select(
        SharedString::from(format!("condition-holds-{}", card.path.id)),
        if not { "does not hold" } else { "holds" },
    )
    .on_click(move |event: &ClickEvent, window, cx| {
        let choices = [(false, "holds"), (true, "does not hold")].map(|(value, label)| {
            let (set, when) = (set.clone(), holds_when.clone());
            Choice::new(label, Some(value == not), move |_, cx| {
                set(cx, when.clone(), value)
            })
        });
        open_choices(choices.into(), event.position(), window, cx);
    });
    h_flex()
        .flex_wrap()
        .gap_2()
        .child(Label::new("If").weight(FontWeight::MEDIUM))
        .child(div().w(rems_from_px(176.0_f32)).child(kind_button))
        .child(div().w(rems_from_px(160.0_f32)).child(holds_button))
}

fn condition_fields(
    card: &PathCard,
    own: &PathFields,
    when: &Condition,
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> AnyElement {
    let problems = condition_problems_by_field(when);
    let on_command = matches!(when, Condition::Command { .. });
    let text = |label: &'static str,
                help: Option<&'static str>,
                field: &LiveField,
                problem: Option<String>| {
        v_flex()
            .gap_1p5()
            .child(labelled(
                label,
                None,
                help.map(Into::into),
                input_box(&field.editor, cx),
            ))
            .children(problem.map(error_line))
    };
    let fields = if on_command {
        vec![text(
            "Command",
            Some(
                "Runs in the task's folder, like a check command. The condition holds when it exits with code 0.",
            ),
            &own.run,
            problems.run,
        )]
    } else {
        vec![
            text(
                "File",
                Some("A markdown file in the task's folder."),
                &own.file,
                problems.file,
            ),
            text(
                "Section (optional)",
                Some("Only the lines under this heading count. Empty: the whole file."),
                &own.section,
                problems.section,
            ),
            text(
                "Text to look for",
                Some("Upper and lower case do not matter."),
                &own.contains,
                problems.contains,
            ),
        ]
    };
    v_flex()
        .gap_3()
        .child(condition_selects(card, when, pipeline))
        .children(fields)
        .into_any_element()
}
