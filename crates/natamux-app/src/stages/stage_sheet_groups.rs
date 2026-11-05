//! The groups of the stage sheet (layout-b 5.3): Basic (name, approval, agent, end the agent)
//! and Prompt on the Basic tab; Commands and files, and Options on the Advanced tab.

use gpui::{AnyElement, ClickEvent, FocusHandle, Focusable};
use natamux_proto::stages::schema::{Stage, Step};
use ui::prelude::*;

use super::editor_fields::{TextField, set_text};
use super::editor_find::find_stage;
use super::sheet_rules::{EndAgent, NOT_USED, agent_help, sheet_shows};
use super::stage_fields_view::{Choice, LiveField, SELECT_WIDTH, change, open_choices, select};
use super::stage_options::{approval_row, end_agent_row, option_rows, terminal_one_row};
use super::stage_sheet::StageSheet;
use crate::agents::agent_choices::{agent_label, custom_agents, pick_list};
use crate::components::form::SettingsInput;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, SettingsRow};
use crate::components::settings_controls::{SheetField, changed_mark};

/// Two fields side by side, 12 px apart; a missing one leaves its half empty.
fn two_columns(left: Option<AnyElement>, right: Option<AnyElement>) -> AnyElement {
    let half = |field: Option<AnyElement>| div().flex_1().min_w_0().children(field);
    h_flex()
        .w_full()
        .items_start()
        .gap(rems_from_px(12.0_f32))
        .child(half(left))
        .child(half(right))
        .into_any_element()
}

/// A row of the Commands and files card: padding 10 12, the fields at the top.
fn field_row(child: AnyElement) -> AnyElement {
    div()
        .w_full()
        .px(rems_from_px(12.0_f32))
        .pt(rems_from_px(10.0_f32))
        .child(child)
        .into_any_element()
}

impl StageSheet {
    pub(super) fn basic_groups(
        &mut self,
        stage: &Stage,
        steps: &[Step],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let shows = sheet_shows(stage, steps);
        let mut rows = vec![
            self.name_row(stage, cx),
            approval_row(stage, &self.pipeline, cx),
        ];
        if shows.agent {
            rows.push(self.agent_row(stage, steps));
        }
        match shows.end_agent {
            EndAgent::Switch => rows.push(end_agent_row(stage, &self.pipeline, cx)),
            EndAgent::TerminalOne => rows.push(terminal_one_row()),
            EndAgent::Hidden => {}
        }
        let mut groups = vec![
            SettingsGroup::new("Basic")
                .child(SettingsCard::new(SeparatorInset::Text).children(rows))
                .into_any_element(),
        ];
        if shows.prompt {
            groups.push(self.render_prompt(stage, cx));
        }
        groups
    }

    pub(super) fn advanced_groups(
        &mut self,
        stage: &Stage,
        steps: &[Step],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let rows = self.command_rows(stage, steps);
        let mut groups = vec![
            SettingsGroup::new("Commands and files")
                .child(SettingsCard::new(SeparatorInset::Text).children(rows))
                .into_any_element(),
        ];
        let options = option_rows(stage, &sheet_shows(stage, steps), &self.pipeline, cx);
        if !options.is_empty() {
            groups.push(
                SettingsGroup::new("Options")
                    .child(SettingsCard::new(SeparatorInset::Text).children(options))
                    .into_any_element(),
            );
        }
        groups
    }

    fn name_row(&self, stage: &Stage, cx: &App) -> AnyElement {
        let (pipeline, id) = (self.pipeline.clone(), stage.id.clone());
        let mark = changed_mark(
            "stage-name-default",
            stage.name.is_some(),
            move |_, cx| {
                change(&pipeline, cx, |steps| {
                    set_text(steps, &id, TextField::Name, "")
                })
            },
            cx,
        );
        SettingsRow::new("stage-name", "Name")
            .control(mark)
            .control(SettingsInput::new(&self.fields.name.editor).width(240.))
            .into_any_element()
    }

    fn agent_row(&self, stage: &Stage, steps: &[Step]) -> AnyElement {
        let picked = stage.agent.clone();
        let text = picked
            .as_deref()
            .map_or_else(|| "Task's agent".to_string(), |id| agent_label(Some(id)));
        let (pipeline, id) = (self.pipeline.clone(), stage.id.clone());
        let button = select(SharedString::from(format!("agent-{id}")), text).on_click(
            move |event: &ClickEvent, window, cx| {
                let pick = |value: String, label: String| {
                    let (pipeline, id) = (pipeline.clone(), id.clone());
                    let checked = picked.as_deref().unwrap_or("") == value;
                    Choice::new(label, Some(checked), move |_, cx| {
                        change(&pipeline, cx, |steps| {
                            set_text(steps, &id, TextField::Agent, &value)
                        })
                    })
                };
                let mut choices = vec![pick(String::new(), "Task's agent".to_owned())];
                choices.extend(
                    pick_list(&custom_agents())
                        .into_iter()
                        .map(|(value, label)| pick(value, label)),
                );
                open_choices(choices, event.position(), window, cx);
            },
        );
        SettingsRow::new("stage-agent", "Agent")
            .help(agent_help(steps, &stage.id))
            .stacked(true)
            .control(v_flex().w(rems_from_px(SELECT_WIDTH)).child(button))
            .into_any_element()
    }

    /// The text fields on the open tab, in drawn order, for the Tab ring.
    pub(super) fn field_stops(&self, advanced: bool, cx: &App) -> Vec<FocusHandle> {
        let Some(stage) = find_stage(self.pipeline.read(cx).steps(), &self.id) else {
            return Vec::new();
        };
        let shows = sheet_shows(stage, self.pipeline.read(cx).steps());
        let fields = &self.fields;
        let wanted = if advanced {
            vec![
                (shows.args, &fields.args),
                (shows.command, &fields.command),
                (true, &fields.check),
                (shows.output, &fields.output),
                (shows.sections, &fields.sections),
                (true, &fields.done_when),
            ]
        } else {
            vec![(true, &fields.name), (shows.prompt, &fields.prompt)]
        };
        wanted
            .into_iter()
            .filter(|(shown, _)| *shown)
            .map(|(_, field)| field.editor.focus_handle(cx))
            .collect()
    }

    /// [Arguments | Command], [Check command | Output file], Required sections, Finished when.
    fn command_rows(&self, stage: &Stage, steps: &[Step]) -> Vec<AnyElement> {
        let shows = sheet_shows(stage, steps);
        let fields = &self.fields;
        let input = |field: &LiveField| SettingsInput::new(&field.editor).into_any_element();
        let not_used = |unused: bool| unused.then_some(NOT_USED);
        let args = shows.args.then(|| {
            SheetField::new("Arguments", input(&fields.args))
                .help(not_used(shows.args_unused))
                .into_any_element()
        });
        let command = shows.command.then(|| {
            SheetField::new("Command", input(&fields.command))
                .help(not_used(shows.command_unused))
                .into_any_element()
        });
        let check = SheetField::new("Check command", input(&fields.check)).into_any_element();
        let output = shows
            .output
            .then(|| SheetField::new("Output file", input(&fields.output)).into_any_element());
        let mut rows = Vec::new();
        if args.is_some() || command.is_some() {
            rows.push(field_row(two_columns(args, command)));
        }
        rows.push(field_row(two_columns(Some(check), output)));
        if shows.sections {
            let file = stage.output.clone().unwrap_or_default();
            let area = SettingsInput::new(&fields.sections.editor).area();
            let field = SheetField::new("Required sections", area)
                .help(Some(format!("{file} must have each.")));
            rows.push(field_row(field.into_any_element()));
        }
        let finished = SheetField::new("Finished when", input(&fields.done_when));
        rows.push(field_row(finished.into_any_element()));
        rows
    }
}
