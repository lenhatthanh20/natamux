//! The checkboxes of a stage (`StageOptions.tsx`): approval, workflow, CI, end the terminal, and
//! a terminal of its own. Each has a Default / Changed mark and a note.

use gpui::{AnyElement, Entity};
use natamux_proto::stages::capabilities::{can_end_shell, checks_on_ci};
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::stages::terminal_lane::terminal_of_stage;
use ui::{ButtonLike, Checkbox, prelude::*};

use super::editor_fields::{
    always_waits, default_ci, default_gate, default_own_shell, default_workflow, is_default_gate,
    offers_own_shell, set_ci, set_end_shell, set_gate, set_own_shell, set_workflow,
};
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{change, default_mark, help_line};

const IN_SHELL_ONE: &str = "This stage runs in Terminal 1, which cannot be closed. Terminal 1 \
                            keeps the task's agent and its session.";

/// Shell 1 keeps the task's agent and its session, so endShell never closes it. A testing stage
/// with a command runs it in a shell of its own, which it may end (`endsShell1`).
fn ends_shell_one(steps: &[Step], stage: &Stage) -> bool {
    if stage.kind == StageKind::Testing && stage.command.is_some() {
        return false;
    }
    terminal_of_stage(steps, &stage.id) == "1"
}

/// The button's 4 px padding, the 20 px box and the 8 px gap: a note starts under the label.
const NOTE_INDENT: f32 = 32.;

/// One checkbox with its label, mark and note.
/// What a checkbox row does with its new value.
type Toggle = Box<dyn Fn(bool, &mut Window, &mut App)>;

struct Row {
    id: String,
    label: &'static str,
    note: Option<&'static str>,
    on: bool,
    disabled: bool,
    mark: Option<AnyElement>,
    toggle: Toggle,
}

fn render(row: Row, _cx: &App) -> AnyElement {
    let Row {
        id,
        label,
        note,
        on,
        disabled,
        mark,
        toggle,
    } = row;
    v_flex()
        .gap_1()
        .child(
            h_flex()
                .min_h(rems_from_px(28.0_f32))
                .flex_wrap()
                .gap_2()
                .child(
                    ButtonLike::new(SharedString::from(id.clone()))
                        .disabled(disabled)
                        .on_click(move |_, window, cx| toggle(!on, window, cx))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Checkbox::new(
                                        SharedString::from(format!("{id}-box")),
                                        on.into(),
                                    )
                                    .disabled(disabled)
                                    .visualization_only(true),
                                )
                                .child(
                                    div()
                                        .debug_selector(|| "option-label".into())
                                        .child(Label::new(label).size(LabelSize::Small)),
                                ),
                        ),
                )
                .children(mark),
        )
        .children(note.map(|note| {
            div().pl(px(NOTE_INDENT)).child(
                div()
                    .debug_selector(|| "option-note".into())
                    .child(help_line(note)),
            )
        }))
        .into_any_element()
}

/// The checkboxes of `stage`, in the order of the details. `Some` of a note for a stage that
/// runs in Terminal 1 replaces the end-the-terminal box.
pub fn stage_options(
    stage: &Stage,
    steps: &[Step],
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> Vec<AnyElement> {
    let mut rows = vec![approval(stage, pipeline, cx)];
    if stage.kind == StageKind::Agent {
        rows.push(workflow(stage, pipeline, cx));
    }
    if checks_on_ci(stage) {
        rows.push(ci(stage, pipeline, cx));
    }
    if can_end_shell(stage) {
        if ends_shell_one(steps, stage) {
            rows.push(help_line(IN_SHELL_ONE).into_any_element());
        } else {
            rows.push(end_shell(stage, pipeline, cx));
        }
    }
    if offers_own_shell(steps, &stage.id) {
        rows.push(own_shell(stage, pipeline, cx));
    }
    rows
}

fn approval(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let always = always_waits(stage);
    let release = stage.kind == StageKind::Release;
    let note = if always {
        Some("This stage is a check by a person.")
    } else if release {
        Some("Unchecked, the pull request opens when the stage starts.")
    } else {
        None
    };
    let id = stage.id.clone();
    let kind = stage.kind;
    let mark = (!always).then(|| {
        let (pipeline, id) = (pipeline.clone(), id.clone());
        default_mark(
            format!("gate-default-{}", stage.id),
            is_default_gate(stage),
            move |_, cx| {
                change(&pipeline, cx, |steps| {
                    set_gate(steps, &id, default_gate(&id, kind))
                })
            },
            cx,
        )
    });
    let pipeline = pipeline.clone();
    let stage_id = id.clone();
    render(
        Row {
            id: format!("gate-{id}"),
            label: if release {
                "Open the pull request when I click"
            } else {
                "Wait for my approval"
            },
            note,
            on: always || stage.gate == Gate::Manual,
            disabled: always,
            mark,
            toggle: Box::new(move |on, _, cx| {
                let gate = if on { Gate::Manual } else { Gate::Auto };
                change(&pipeline, cx, |steps| set_gate(steps, &stage_id, gate));
            }),
        },
        cx,
    )
}

/// A checkbox that turns a flag of the stage on or off, with its default.
fn flag(
    stage: &Stage,
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
    spec: (&'static str, &'static str, &'static str, bool, bool),
    set: fn(&[Step], &str, bool) -> Vec<Step>,
) -> AnyElement {
    let (key, label, note, on, default) = spec;
    let (toggle_pipeline, toggle_id) = (pipeline.clone(), stage.id.clone());
    let (reset_pipeline, reset_id) = (pipeline.clone(), stage.id.clone());
    let mark = default_mark(
        format!("{key}-default-{}", stage.id),
        on == default,
        move |_, cx| change(&reset_pipeline, cx, |steps| set(steps, &reset_id, default)),
        cx,
    );
    render(
        Row {
            id: format!("{key}-{}", stage.id),
            label,
            note: Some(note),
            on,
            disabled: false,
            mark: Some(mark),
            toggle: Box::new(move |on, _, cx| {
                change(&toggle_pipeline, cx, |steps| set(steps, &toggle_id, on))
            }),
        },
        cx,
    )
}

fn workflow(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let spec = (
        "workflow",
        "Tasks can run it as a workflow",
        "Each task gets a Run with a workflow checkbox on this stage, off by default. Only Claude \
         Code runs workflows; they are more accurate and use many more tokens.",
        stage.workflow == Some(true),
        default_workflow(&stage.id, stage.kind),
    );
    flag(stage, pipeline, cx, spec, set_workflow)
}

fn ci(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let spec = (
        "ci",
        "Tasks can check it on CI",
        "Each task gets a Check on CI checkbox on this stage, off by default. The work is pushed \
         to a natamux-ci branch, so the CI workflows must run on a push to natamux-ci/**.",
        stage.ci == Some(true),
        default_ci(&stage.id),
    );
    flag(stage, pipeline, cx, spec, set_ci)
}

fn end_shell(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let spec = (
        "end-shell",
        "End the agent and close its terminal when the stage is done",
        "Frees the memory of the stage. A next stage in the same terminal starts a new agent \
         session. The last stage is left to the end of the task, which keeps its screen.",
        stage.end_shell == Some(true),
        false,
    );
    flag(stage, pipeline, cx, spec, set_end_shell)
}

fn own_shell(stage: &Stage, pipeline: &Entity<PipelineEditor>, cx: &App) -> AnyElement {
    let spec = (
        "own-shell",
        "Run in a terminal of its own",
        "Runs in Terminal 5 with a new session, apart from the task's agent in Terminal 1. Off: \
         runs in Terminal 1 with the task's agent and its session.",
        stage.own_shell == Some(true),
        default_own_shell(&stage.id),
    );
    flag(stage, pipeline, cx, spec, set_own_shell)
}

/// A plain checkbox with a label, no mark and no note.
pub fn checkbox_row(
    id: String,
    label: &'static str,
    on: bool,
    toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    render(
        Row {
            id,
            label,
            note: None,
            on,
            disabled: false,
            mark: None,
            toggle: Box::new(toggle),
        },
        cx,
    )
}

#[cfg(test)]
#[path = "stage_options_tests.rs"]
mod tests;
