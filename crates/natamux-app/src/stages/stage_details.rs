//! The details pane of the Stages tab (`StageDetails.tsx`): in a bordered box, the stage picker
//! while adding, else the branch details, else the settings of the picked stage. The text fields
//! are editors made when the pick changes and kept otherwise.

use gpui::{AnyElement, ClickEvent, Entity, FocusHandle, Subscription, rems};
use natamux_proto::projects::repo::SkillList;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use ui::{ContextMenu, prelude::*};

use super::branch_details_fields::BranchFields;
use super::editor_fields::{
    TextField, runs_in_own_shell, runs_in_shell_one, set_text, takes_own_agent, takes_prompt,
};
use super::editor_labels::kind_label;
use super::kind_icon::kind_icon;
use super::pipeline_editor::PipelineEditor;
use super::stage_details_fields::StageFields;
use super::stage_fields_view::{
    Choice, change, default_mark, issue_list, labelled, open_choices, select,
};
use super::stage_options::stage_options;
use super::stage_picker::StagePicker;
use crate::agents::agent_choices::{AGENT_CHOICES, agent_label};
use crate::components::form::input_box;
use crate::components::tab_order;

/// A command or arguments set in the file on a stage that does not run them stay visible, so a
/// shared file hides nothing.
const NOT_USED: &str = "This stage does not use it. Clear it to remove it from the file.";

pub struct DetailsPane {
    pub(super) editor: Entity<PipelineEditor>,
    pub(super) skills: Option<Result<SkillList, String>>,
    pub(super) stage_fields: Option<StageFields>,
    branch_fields: Option<BranchFields>,
    picker: Option<Entity<StagePicker>>,
    pub(super) skill_menu: Option<(Entity<ContextMenu>, Subscription)>,
    /// The fields drawn last, in order, for the Tab ring of the dialog.
    stops: Vec<FocusHandle>,
    _watch: Subscription,
}

impl DetailsPane {
    pub fn new(
        editor: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let watch = cx.observe_in(&editor, window, |this, _, window, cx| {
            this.refresh(window, cx)
        });
        let mut pane = DetailsPane {
            editor,
            skills: None,
            stage_fields: None,
            branch_fields: None,
            picker: None,
            skill_menu: None,
            stops: Vec::new(),
            _watch: watch,
        };
        pane.refresh(window, cx);
        pane
    }

    /// The fields on screen, in order: the picker's, or the details'.
    pub fn tab_stops(&self, cx: &App) -> Vec<FocusHandle> {
        match &self.picker {
            Some(picker) if self.editor.read(cx).picking => picker.read(cx).tab_stops(),
            _ => self.stops.clone(),
        }
    }

    /// The search field of the stage picker, while it is open.
    pub fn search_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.picker
            .as_ref()
            .map(|picker| picker.read(cx).search_focus(cx))
    }

    /// Esc closes the top layer first: the skill menu, then the stage picker.
    pub fn closes_layer(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.close_skills(cx) {
            return true;
        }
        if self.editor.read(cx).picking {
            self.editor
                .update(cx, |editor, cx| editor.set_picking(false, cx));
            return true;
        }
        false
    }

    /// Makes the fields of the picked stage or branch, and shows the draft in the ones without
    /// the focus.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (steps, picking, branch, stage) = {
            let editor = self.editor.read(cx);
            (
                editor.steps().to_vec(),
                editor.picking,
                editor.selected_branch().map(|branch| branch.id.clone()),
                editor.selected_stage().map(|stage| stage.id.clone()),
            )
        };
        if !picking {
            self.picker = None;
        } else if self.picker.is_none() {
            let editor = self.editor.clone();
            let picker = cx.new(|cx| StagePicker::new(editor, window, cx));
            let focus = picker.read(cx).search_focus(cx);
            window.focus(&focus, cx);
            self.picker = Some(picker);
        }
        self.sync_stage(&steps, stage, window, cx);
        self.sync_branch(&steps, branch, window, cx);
        cx.notify();
    }

    fn sync_stage(
        &mut self,
        steps: &[Step],
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = id else {
            self.stage_fields = None;
            return;
        };
        match &self.stage_fields {
            Some(fields) if fields.id == id => fields.sync(steps, window, cx),
            _ => {
                self.close_skills(cx);
                self.stage_fields = Some(StageFields::new(&self.editor, &id, window, cx));
            }
        }
    }

    fn sync_branch(
        &mut self,
        steps: &[Step],
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = id else {
            self.branch_fields = None;
            return;
        };
        match self.branch_fields.as_mut() {
            Some(fields) if fields.id() == id => fields.sync(&self.editor, steps, window, cx),
            _ => self.branch_fields = Some(BranchFields::new(&self.editor, &id, window, cx)),
        }
    }

    pub(super) fn branch_fields(&self) -> Option<&BranchFields> {
        self.branch_fields.as_ref()
    }
}

impl Render for DetailsPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (steps, picking, branch, stage, issues) = {
            let editor = self.editor.read(cx);
            let id = editor.selected_id();
            (
                editor.steps().to_vec(),
                editor.picking,
                editor.selected_branch().cloned(),
                editor.selected_stage().cloned(),
                editor.issues_of(id.as_deref()),
            )
        };
        tab_order::begin();
        let body = match (&self.picker, picking, branch, stage) {
            (Some(picker), true, _, _) => picker.clone().into_any_element(),
            (_, _, Some(branch), _) => self.render_branch(&branch, &steps, &issues, cx),
            (_, _, _, Some(stage)) => self.render_stage(&stage, &steps, &issues, cx),
            _ => Label::new("Pick a stage to see its settings.")
                .color(Color::Muted)
                .into_any_element(),
        };
        self.stops = tab_order::end();
        div()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(body)
    }
}

impl DetailsPane {
    fn render_stage(
        &self,
        stage: &Stage,
        steps: &[Step],
        issues: &[PipelineIssue],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(fields) = &self.stage_fields else {
            return div().into_any_element();
        };
        let own_agent = takes_own_agent(steps, &stage.id);
        let mut body = v_flex()
            .gap_4()
            .child(stage_header(stage, cx))
            .children(issue_list(issues, cx))
            .child(self.name_field(stage, cx))
            .children(stage_options(stage, steps, &self.editor, cx));
        if own_agent {
            body = body.child(self.agent_field(stage, steps, cx));
        }
        if own_agent || stage.args.is_some() {
            let help = if own_agent {
                "Words added to the agent's command line, such as --model sonnet."
            } else {
                NOT_USED
            };
            body = body.child(labelled(
                "Arguments",
                None,
                Some(help.into()),
                input_box(&fields.args.editor, cx),
            ));
        }
        if takes_prompt(stage) {
            body = body.child(self.render_prompt(stage, cx));
        }
        body.children(self.command_fields(stage, cx))
            .into_any_element()
    }

    fn name_field(&self, stage: &Stage, cx: &App) -> impl IntoElement {
        let Some(fields) = &self.stage_fields else {
            return div().into_any_element();
        };
        let (pipeline, id) = (self.editor.clone(), stage.id.clone());
        let mark = default_mark(
            "name-default",
            stage.name.is_none(),
            move |_, cx| {
                change(&pipeline, cx, |steps| {
                    set_text(steps, &id, TextField::Name, "")
                })
            },
            cx,
        );
        labelled("Name", Some(mark), None, input_box(&fields.name.editor, cx)).into_any_element()
    }

    fn agent_field(&self, stage: &Stage, steps: &[Step], _cx: &App) -> AnyElement {
        let help = if runs_in_shell_one(steps, &stage.id) {
            "With the task's agent, this stage runs in Terminal 1. Another agent or arguments run in \
             a terminal of its own, so Terminal 1 keeps the task's agent."
        } else if runs_in_own_shell(steps, &stage.id) {
            "This stage runs in a terminal of its own. With the task's agent it uses Terminal 5; \
             another agent or arguments use a terminal from Terminal 6. Uncheck Run in a terminal \
             of its own to run it in Terminal 1."
        } else {
            "This stage starts its own agent beside the one in Terminal 1, so it may use another."
        };
        let picked = stage.agent.clone();
        let text = picked
            .as_deref()
            .map_or_else(|| "Task's agent".to_string(), |id| agent_label(Some(id)));
        let (pipeline, id) = (self.editor.clone(), stage.id.clone());
        let button = select(SharedString::from(format!("agent-{id}")), text).on_click(
            move |event: &ClickEvent, window, cx| {
                let pick = |value: &'static str, label: &'static str| {
                    let (pipeline, id) = (pipeline.clone(), id.clone());
                    let checked = picked.as_deref().unwrap_or("") == value;
                    Choice::new(label, Some(checked), move |_, cx| {
                        change(&pipeline, cx, |steps| {
                            set_text(steps, &id, TextField::Agent, value)
                        })
                    })
                };
                let mut choices = vec![pick("", "Task's agent")];
                choices.extend(
                    AGENT_CHOICES
                        .iter()
                        .map(|choice| pick(choice.id, choice.label)),
                );
                open_choices(choices, event.position(), window, cx);
            },
        );
        labelled("Agent", None, Some(help.into()), button).into_any_element()
    }

    /// Command, check, output, sections and finished when.
    fn command_fields(&self, stage: &Stage, cx: &App) -> Vec<AnyElement> {
        let Some(fields) = &self.stage_fields else {
            return Vec::new();
        };
        let catalog = catalog_entry(&stage.id);
        let mut rows = Vec::new();
        if stage.kind == StageKind::Testing || stage.command.is_some() {
            let help = if stage.kind != StageKind::Testing {
                NOT_USED
            } else if catalog.is_some_and(|entry| entry.needs_command) {
                "Required. The stage passes when this command passes."
            } else {
                "Empty: the agent writes the tests and runs them, as the prompt says."
            };
            rows.push(
                labelled(
                    "Command",
                    None,
                    Some(help.into()),
                    input_box(&fields.command.editor, cx),
                )
                .into_any_element(),
            );
        }
        rows.push(
            labelled(
                "Check command",
                None,
                Some("Runs in a shell in the task's folder after the stage reports done. Exit code 0 passes. Empty: none.".into()),
                input_box(&fields.check.editor, cx),
            )
            .into_any_element(),
        );
        if stage.kind == StageKind::Agent {
            let help = "A markdown file the stage must write, such as notes.md. Empty: none.";
            rows.push(
                labelled(
                    "Output file",
                    None,
                    Some(help.into()),
                    input_box(&fields.output.editor, cx),
                )
                .into_any_element(),
            );
        }
        if let (StageKind::Agent, Some(output)) = (stage.kind, &stage.output) {
            let help = format!("One heading per line. {output} must have each.");
            rows.push(
                labelled(
                    "Required sections",
                    None,
                    Some(help.into()),
                    input_box(&fields.sections.editor, cx),
                )
                .into_any_element(),
            );
        }
        rows.push(
            labelled(
                "Finished when",
                None,
                Some("Shown on the stage card. Empty: the sentence in the field.".into()),
                input_box(&fields.done_when.editor, cx),
            )
            .into_any_element(),
        );
        rows
    }
}

fn stage_header(stage: &Stage, cx: &App) -> impl IntoElement {
    let sentence =
        catalog_entry(&stage.id).map_or("A stage of your own", |entry| entry.description);
    let dot = || {
        Label::new("\u{b7}")
            .size(LabelSize::Small)
            .color(Color::Muted)
    };
    v_flex()
        .gap_0p5()
        .child(
            h_flex()
                .gap_2()
                .child(
                    kind_icon(stage.kind)
                        .size(IconSize::Medium)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(stage_name(stage))
                        .size(LabelSize::Custom(rems(15. / 16.)))
                        .weight(gpui::FontWeight::SEMIBOLD)
                        .truncate(),
                ),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap_1()
                .child(
                    Label::new(kind_label(stage.kind))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(dot())
                .child(
                    Label::new(stage.id.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .child(dot())
                .child(
                    Label::new(sentence)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
}
