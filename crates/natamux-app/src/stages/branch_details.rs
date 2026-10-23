//! The details of a branch (`BranchDetails.tsx`): its name, and a card for each path with its
//! condition and stages.

use gpui::{AnyElement, ClickEvent, Entity, FontWeight, rems};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{BranchPath, Condition, StageBranch, StageKind, Step};
use ui::prelude::*;

use super::branch_details_fields::{BranchFields, PathFields};
use super::editor_branches::{
    ConditionType, add_path, condition_problems_by_field, remove_path, retype_condition,
    set_otherwise, set_path_condition,
};
use super::editor_catalog::{add_catalog_stage, catalog_choices};
use super::editor_draft::MAX_PATHS;
use super::kind_icon::{branch_icon, kind_icon, remove_icon};
use super::pipeline_editor::PipelineEditor;
use super::stage_details::DetailsPane;
use super::stage_fields_view::{
    Choice, LiveField, change, error_line, field_label, help_line, issue_list, labelled,
    open_choices, select,
};
use super::stage_options::checkbox_row;
use crate::components::confirm::Confirm;
use crate::components::form::input_box;

impl DetailsPane {
    pub(super) fn render_branch(
        &self,
        branch: &StageBranch,
        steps: &[Step],
        issues: &[PipelineIssue],
        cx: &App,
    ) -> AnyElement {
        let Some(fields) = self.branch_fields() else {
            return div().into_any_element();
        };
        let name = super::editor_labels::branch_name(branch);
        let pipeline = &self.editor;
        let last = branch.paths.last();
        let cards = branch.paths.iter().enumerate().map(|(index, path)| {
            let is_last = index + 1 == branch.paths.len();
            path_card(
                &PathCard {
                    branch,
                    path,
                    index,
                    is_last,
                },
                fields,
                steps,
                pipeline,
                cx,
            )
        });
        let (add_pipeline, branch_id) = (pipeline.clone(), branch.id.clone());
        let add = Button::new("add-path", "Add path")
            .style(ButtonStyle::Outlined)
            .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
            .on_click(move |_, _, cx| {
                change(&add_pipeline, cx, |steps| add_path(steps, &branch_id))
            });
        let skip = last.is_some_and(|path| path.when.is_some());
        v_flex()
            .gap_4()
            .child(branch_header(branch, &name, cx))
            .children(issue_list(issues, cx))
            .child(labelled(
                "Branch name",
                None,
                None,
                input_box(&fields.name.editor, cx),
            ))
            .child(v_flex().gap_3().children(cards))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .when(branch.paths.len() < MAX_PATHS, |this| this.child(add))
                    .when(skip, |this| {
                        this.child(help_line(
                            "No path holds: the task skips this branch and goes on.",
                        ))
                    }),
            )
            .into_any_element()
    }
}

fn branch_header(branch: &StageBranch, name: &str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            h_flex()
                .gap_2()
                .child(branch_icon().size(IconSize::Medium).color(Color::Accent))
                .child(
                    Label::new(name.to_owned())
                        .size(LabelSize::Custom(rems(15. / 16.)))
                        .weight(FontWeight::SEMIBOLD)
                        .truncate(),
                ),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap_1()
                .child(
                    Label::new("Branch \u{b7}")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(branch.id.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .child(
                    Label::new(
                        "\u{b7} A task takes the first path whose condition holds, and runs only \
                         its stages.",
                    )
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                ),
        )
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
    steps: &[Step],
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
        .rounded_lg()
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
        .child(stages_of_path(card, &label, steps, pipeline, cx))
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
        .child(div().w(px(176.)).child(kind_button))
        .child(div().w(px(160.)).child(holds_button))
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

fn stages_of_path(
    card: &PathCard,
    label: &str,
    steps: &[Step],
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> impl IntoElement {
    let hover = cx.theme().colors().ghost_element_hover;
    let rows = card.path.stages.iter().map(|stage| {
        let (pipeline, id) = (pipeline.clone(), stage.id.clone());
        h_flex()
            .id(SharedString::from(format!("path-stage-{}", stage.id)))
            .gap_2()
            .px_1p5()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, _, cx| pipeline.update(cx, |pipeline, cx| pipeline.select(&id, cx)))
            .child(
                kind_icon(stage.kind)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(Label::new(stage_name(stage)).truncate())
    });
    let (add_pipeline, path_id) = (pipeline.clone(), card.path.id.clone());
    let choices: Vec<String> = catalog_choices(steps)
        .into_iter()
        .filter(|entry| !matches!(entry.stage.kind, StageKind::Context | StageKind::Release))
        .map(|entry| entry.stage.id.clone())
        .collect();
    let add = select(
        SharedString::from(format!("add-stage-{path_id}")),
        "Add a stage\u{2026}",
    )
    .on_click(move |event: &ClickEvent, window, cx| {
        let choices = choices
            .iter()
            .filter_map(|id| catalog_entry(id))
            .map(|entry| {
                let (pipeline, path_id, id) = (
                    add_pipeline.clone(),
                    path_id.clone(),
                    entry.stage.id.clone(),
                );
                Choice::new(stage_name(&entry.stage), None, move |_, cx| {
                    change(&pipeline, cx, |steps| {
                        add_catalog_stage(steps, &id, Some(&path_id))
                    })
                })
            })
            .collect();
        open_choices(choices, event.position(), window, cx);
    })
    .tooltip(ui::Tooltip::text(format!("Add a stage to {label}")));
    v_flex()
        .gap_1p5()
        .child(field_label("Stages, one after another"))
        .when(card.path.stages.is_empty(), |this| {
            this.child(help_line("No stage yet. Add one below."))
        })
        .child(v_flex().gap_0p5().children(rows))
        .child(add)
}
