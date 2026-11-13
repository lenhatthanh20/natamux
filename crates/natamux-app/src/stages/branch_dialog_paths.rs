//! The path groups of the branch sheet (layout-b 5.6): name, condition ("If [type] [holds]" and
//! its fields), the Otherwise switch on the last path, and Remove path. They edit the dialog's
//! own copy of the draft (`local`), so nothing reaches the real draft before the dialog is saved.

use gpui::{AnyElement, ClickEvent, Entity, Focusable};
use natamux_proto::stages::schema::{BranchPath, Condition, StageBranch};
use ui::{Switch, ToggleState, prelude::*};

use super::branch_dialog_fields::{BranchFields, PathFields};
use super::editor_branches::{
    ConditionType, condition_problems_by_field, remove_path, retype_condition, set_otherwise,
    set_path_condition,
};
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{Choice, LiveField, change, open_choices, select};
use crate::components::chips::label_chip;
use crate::components::confirm::Confirm;
use crate::components::form::SettingsInput;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, SettingsRow};
use crate::components::settings_controls::SheetField;
use crate::components::tab_order;

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

/// A text box of the sheet, `width` px or the full width; drawn now, so it is a Tab stop now.
fn input(field: &LiveField, width: Option<f32>, cx: &App) -> SettingsInput {
    tab_order::push(&field.editor.focus_handle(cx));
    let input = SettingsInput::new(&field.editor);
    match width {
        Some(width) => input.width(width),
        None => input,
    }
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
    let otherwise = path.when.is_none();
    let mut rows = vec![
        SettingsRow::new(SharedString::from(format!("path-name-{}", path.id)), "Name")
            .control(input(&own.name, Some(240.), cx))
            .into_any_element(),
    ];
    match &path.when {
        None => rows.push(
            SettingsRow::new(SharedString::from(format!("path-if-{}", path.id)), "If")
                .help("Runs when no earlier path holds.")
                .control(label_chip("Otherwise"))
                .into_any_element(),
        ),
        Some(when) => rows.extend(condition_rows(card, own, when, pipeline, cx)),
    }
    if *is_last && branch.paths.len() > 1 {
        let (pipeline, branch_id, path_id) = (pipeline.clone(), branch.id.clone(), path.id.clone());
        let switch = Switch::new(
            SharedString::from(format!("otherwise-{}", path.id)),
            otherwise.into(),
        )
        .on_click(move |_: &ToggleState, _, cx| {
            change(&pipeline, cx, |steps| {
                set_otherwise(steps, &branch_id, &path_id, !otherwise)
            })
        });
        rows.push(
            SettingsRow::new("path-otherwise", "Make this the Otherwise path")
                .control(switch)
                .into_any_element(),
        );
    }
    let mut group = SettingsGroup::new(label.clone())
        .child(SettingsCard::new(SeparatorInset::Text).children(rows));
    if branch.paths.len() > 1 {
        group = group.action(remove_button(card, &label, pipeline));
    }
    group.into_any_element()
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
    IconButton::new(SharedString::from(id), IconName::Trash)
        .icon_size(IconSize::Small)
        .icon_color(Color::Muted)
        .tooltip(ui::Tooltip::text("Remove path"))
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
                    "Remove",
                    move |_, cx| remove(cx),
                )
                .sheet()
                .open_over(window, cx);
            } else {
                remove(cx);
            }
        })
}

/// "If": the condition type (160) and whether it holds (130).
fn condition_selects(
    card: &PathCard,
    when: &Condition,
    pipeline: &Entity<PipelineEditor>,
) -> AnyElement {
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
    SettingsRow::new(
        SharedString::from(format!("path-if-{}", card.path.id)),
        "If",
    )
    .control(v_flex().w(rems_from_px(160.0_f32)).child(kind_button))
    .control(v_flex().w(rems_from_px(130.0_f32)).child(holds_button))
    .into_any_element()
}

/// The rows of a condition: "If", then the command, or the file, section and text.
fn condition_rows(
    card: &PathCard,
    own: &PathFields,
    when: &Condition,
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> Vec<AnyElement> {
    let problems = condition_problems_by_field(when);
    let mut rows = vec![condition_selects(card, when, pipeline)];
    if matches!(when, Condition::Command { .. }) {
        rows.push(
            SettingsRow::new(
                SharedString::from(format!("path-run-{}", card.path.id)),
                "Command",
            )
            .error(problems.run)
            .control(input(&own.run, Some(300.), cx))
            .into_any_element(),
        );
        return rows;
    }
    let field = |label: &'static str, live: &LiveField, problem: Option<String>| {
        div()
            .flex_1()
            .min_w_0()
            .child(SheetField::new(label, input(live, None, cx)).error(problem))
    };
    rows.push(
        h_flex()
            .w_full()
            .items_start()
            .gap(rems_from_px(12.0_f32))
            .px(rems_from_px(12.0_f32))
            .pt(rems_from_px(10.0_f32))
            .child(field("File", &own.file, problems.file))
            .child(field("Section", &own.section, problems.section))
            .child(field("Text", &own.contains, problems.contains))
            .into_any_element(),
    );
    rows
}
