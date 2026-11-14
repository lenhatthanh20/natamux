//! The switch rows of the stage sheet (T-0313 mockup B): approval and "End the agent when done"
//! in the Stage card, the Terminal 1 line, and the Options group (workflow, CI, a terminal of its
//! own). Each has the changed mark and its Reset before the switch. Rows are 36 px, label 12,
//! help 11, to fit the 296 px column.

use gpui::{AnyElement, Entity};
use natamux_proto::stages::schema::{Gate, Stage, Step};
use ui::{Switch, ToggleState, prelude::*};

use super::editor_fields::{
    always_waits, default_ci, default_gate, default_own_shell, default_workflow, is_default_gate,
    set_ci, set_end_shell, set_gate, set_own_shell, set_workflow,
};
use super::pipeline_editor::PipelineEditor;
use super::sheet_rules::{SheetShows, TERMINAL_ONE, approval_text};
use super::stage_fields_view::change;
use crate::components::button_style::button_icon;
use crate::components::settings_card::SettingsRow;
use crate::components::settings_controls::changed_mark;

type Set = fn(&[Step], &str, bool) -> Vec<Step>;

pub const END_AGENT: &str = "End the agent when done";
pub const END_AGENT_HELP: &str = "Closes its terminal and frees the memory of the stage. A next \
                                  stage in the same terminal starts a new agent session.";

/// One switch row: label, help, the changed mark, the switch.
struct Flag {
    key: &'static str,
    label: &'static str,
    help: Option<&'static str>,
    on: bool,
    default: bool,
    changed: bool,
    disabled: bool,
}

fn flag_row(
    flag: Flag,
    stage: &Stage,
    pipeline: &Entity<PipelineEditor>,
    set: impl Fn(&[Step], &str, bool) -> Vec<Step> + Clone + 'static,
    cx: &App,
) -> AnyElement {
    let id = format!("{}-{}", flag.key, stage.id);
    let (reset_pipeline, reset_id, reset_set) = (pipeline.clone(), stage.id.clone(), set.clone());
    let default = flag.default;
    let mark = changed_mark(
        format!("{id}-default"),
        flag.changed,
        move |_, cx| {
            change(&reset_pipeline, cx, |steps| {
                reset_set(steps, &reset_id, default)
            })
        },
        cx,
    );
    let (pipeline, stage_id, on) = (pipeline.clone(), stage.id.clone(), flag.on);
    let switch = Switch::new(SharedString::from(id.clone()), on.into())
        .disabled(flag.disabled)
        .on_click(move |_: &ToggleState, _, cx| {
            change(&pipeline, cx, |steps| set(steps, &stage_id, !on))
        });
    // The flow tests click the switch by this name.
    #[cfg(test)]
    let switch = {
        let name = format!("{id}-switch");
        div().debug_selector(move || name).child(switch)
    };
    let row = SettingsRow::new(SharedString::from(format!("{id}-row")), flag.label)
        .label_size(LabelSize::Small)
        .spacing(36., 8.)
        .control(mark)
        .control(switch);
    match flag.help {
        Some(help) => row.help_child(small_help(help)),
        None => row,
    }
    .into_any_element()
}

/// The 11 px muted help line of a column row.
pub fn small_help(text: impl Into<SharedString>) -> Label {
    Label::new(text)
        .size(LabelSize::Custom(rems_from_px(11.0_f32)))
        .color(Color::Muted)
}

/// A flag differs from its default.
fn changed(flag: Flag) -> Flag {
    Flag {
        changed: flag.on != flag.default,
        ..flag
    }
}

fn gate_set(on: bool) -> Gate {
    if on { Gate::Manual } else { Gate::Auto }
}

/// "Wait for my approval" ("Open the pull request when I click" on the release stage). A stage
/// that always waits shows it on and off limits.
pub fn approval_row(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let always = always_waits(stage);
    let (label, help) = approval_text(stage);
    let flag = Flag {
        key: "gate",
        label,
        help,
        on: always || stage.gate == Gate::Manual,
        default: default_gate(&stage.id, stage.kind) == Gate::Manual,
        // A stage that always waits has only its default.
        changed: !always && !is_default_gate(stage),
        disabled: always,
    };
    flag_row(
        flag,
        stage,
        pipeline,
        |steps, id, on| set_gate(steps, id, gate_set(on)),
        cx,
    )
}

/// "End the agent when done": the agent ends and its terminal closes when the stage is done.
pub fn end_agent_row(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let flag = Flag {
        key: "end-shell",
        label: END_AGENT,
        help: Some(END_AGENT_HELP),
        on: stage.end_shell == Some(true),
        default: false,
        changed: stage.end_shell == Some(true),
        disabled: false,
    };
    flag_row(flag, stage, pipeline, set_end_shell as Set, cx)
}

/// The line of a stage that runs in Terminal 1, in place of the End switch: padding 8 12, icon
/// 14, gap 10, 11 px muted text.
pub fn terminal_one_row() -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .px(rems_from_px(12.0_f32))
        .py(rems_from_px(8.0_f32))
        .gap(rems_from_px(10.0_f32))
        .child(
            button_icon("terminal")
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(div().min_w_0().flex_1().child(small_help(TERMINAL_ONE)))
        .into_any_element()
}

/// The rows of the Options group that `shows` has.
pub fn option_rows(
    stage: &Stage,
    shows: &SheetShows,
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    if shows.workflow {
        let flag = Flag {
            key: "workflow",
            label: "Tasks can run it as a workflow",
            help: Some("Claude Code only. More accurate, but many more tokens."),
            on: stage.workflow == Some(true),
            default: default_workflow(&stage.id, stage.kind),
            changed: false,
            disabled: false,
        };
        rows.push(flag_row(
            changed(flag),
            stage,
            pipeline,
            set_workflow as Set,
            cx,
        ));
    }
    if shows.ci {
        let flag = Flag {
            key: "ci",
            label: "Tasks can check it on CI",
            help: Some("Pushes a natamux-ci branch and waits for the run."),
            on: stage.ci == Some(true),
            default: default_ci(&stage.id),
            changed: false,
            disabled: false,
        };
        rows.push(flag_row(changed(flag), stage, pipeline, set_ci as Set, cx));
    }
    if shows.own_terminal {
        let flag = Flag {
            key: "own-shell",
            label: "Run in a terminal of its own",
            help: Some("It gets a terminal apart from Terminal 1."),
            on: stage.own_shell == Some(true),
            default: default_own_shell(&stage.id),
            changed: false,
            disabled: false,
        };
        rows.push(flag_row(
            changed(flag),
            stage,
            pipeline,
            set_own_shell as Set,
            cx,
        ));
    }
    rows
}

#[cfg(test)]
#[path = "stage_options_tests.rs"]
mod tests;
