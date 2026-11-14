//! The right column of the stage sheet (T-0313 mockup B): 296 px on the surface colour, a
//! "Stage" card that never folds (Name, Agent, approval, end the agent or the Terminal 1 line),
//! then the folding groups Done check, Files and arguments, and Options.

use gpui::{AnyElement, ClickEvent, FontWeight, KeyDownEvent};
use natamux_proto::stages::schema::{Stage, Step};
use ui::{ScrollAxes, prelude::*};

use super::advanced_rules::{SheetGroup, changed_in, group_summary};
use super::editor_fields::{TextField, set_text};
use super::sheet_rules::{
    EndAgent, NOT_USED, SheetShows, agent_help, command_at_left, groups_shown, sheet_shows,
};
use super::stage_fields_view::{Choice, change, open_choices, select};
use super::stage_options::{approval_row, end_agent_row, option_rows, terminal_one_row};
use super::stage_sheet::StageSheet;
use super::stage_sheet_groups::{card_line, line_input, sheet_field};
use crate::agents::agent_choices::{agent_label, custom_agents, pick_list};
use crate::components::form::SettingsInput;
use crate::components::scrollbars::scroll_area;
use crate::components::settings_controls::changed_mark;

pub const COLUMN_WIDTH: f32 = 296.;
/// Fields of the column: padding 8 12, inputs 26 high.
const PAD: (f32, f32) = (8., 12.);
const INPUT_H: f32 = 26.;

/// A card of the column: the editor colour on the surface, border, radius 8, a line between
/// two rows.
fn column_card(rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let count = rows.len();
    let mut card = v_flex()
        .w_full()
        .bg(colors.editor_background)
        .border_1()
        .border_color(colors.border_variant)
        .rounded(rems_from_px(8.0_f32))
        .overflow_hidden();
    for (index, row) in rows.into_iter().enumerate() {
        card = card.child(row);
        if index + 1 < count {
            card = card.child(card_line(cx));
        }
    }
    card.into_any_element()
}

/// A group title of the column: h 24, margin 8 0 6, padding 0 4, 12 px semibold muted.
fn title_row() -> Div {
    h_flex()
        .h(rems_from_px(24.0_f32))
        .mt(rems_from_px(8.0_f32))
        .px(rems_from_px(4.0_f32))
        .gap(rems_from_px(6.0_f32))
        .items_center()
}

fn title_label(text: &'static str) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .weight(FontWeight::SEMIBOLD)
        .color(Color::Muted)
}

impl StageSheet {
    pub(super) fn right_column(
        &mut self,
        stage: &Stage,
        steps: &[Step],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shows = sheet_shows(stage, steps);
        let mut body = vec![
            title_row()
                .mb(rems_from_px(6.0_f32))
                .child(title_label("Stage"))
                .into_any_element(),
            div()
                .debug_selector(|| "stage-card".into())
                .child(column_card(self.stage_rows(stage, steps, &shows, cx), cx))
                .into_any_element(),
        ];
        for group in groups_shown(&shows) {
            body.push(self.fold_title(stage, group, cx));
            if self.open[group.index()] {
                let rows = self.group_rows(stage, &shows, group, cx);
                body.push(
                    div()
                        .mt(rems_from_px(6.0_f32))
                        .child(column_card(rows, cx))
                        .into_any_element(),
                );
            }
        }
        let (line, surface) = {
            let colors = cx.theme().colors();
            (colors.border_variant, colors.surface_background)
        };
        let list = v_flex()
            .id("stage-sheet-column")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.column_scroll)
            .pt(rems_from_px(4.0_f32))
            .px(rems_from_px(12.0_f32))
            .pb(rems_from_px(12.0_f32))
            // Each part keeps its height; the column scrolls instead of squeezing a card.
            .children(body.into_iter().map(|part| div().flex_none().child(part)));
        scroll_area(list, ScrollAxes::Vertical, &self.column_scroll, window, cx)
            .debug_selector(|| "stage-sheet-column".into())
            .flex()
            .flex_col()
            .flex_none()
            .w(rems_from_px(COLUMN_WIDTH))
            .h_full()
            .min_h_0()
            .border_l_1()
            .border_color(line)
            .bg(surface)
            .into_any_element()
    }

    /// Name, Agent, approval, and the end-agent switch or the Terminal 1 line.
    fn stage_rows(
        &self,
        stage: &Stage,
        steps: &[Step],
        shows: &SheetShows,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
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
        let name = line_input(&self.fields.name, INPUT_H);
        let mut rows = vec![sheet_field(
            "Name",
            Some(mark.into_any_element()),
            name,
            None,
            PAD,
        )];
        if shows.agent {
            rows.push(self.agent_field(stage, steps));
        }
        rows.push(approval_row(stage, &self.pipeline, cx));
        match shows.end_agent {
            EndAgent::Switch => rows.push(end_agent_row(stage, &self.pipeline, cx)),
            EndAgent::TerminalOne => rows.push(terminal_one_row()),
            EndAgent::Hidden => {}
        }
        rows
    }

    fn agent_field(&self, stage: &Stage, steps: &[Step]) -> AnyElement {
        let picked = stage.agent.clone();
        let text = picked
            .as_deref()
            .map_or_else(|| "Task's agent".to_string(), |id| agent_label(Some(id)));
        let (pipeline, id) = (self.pipeline.clone(), stage.id.clone());
        let button = select(SharedString::from(format!("agent-{id}")), text)
            .height(rems_from_px(INPUT_H).into())
            .on_click(move |event: &ClickEvent, window, cx| {
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
            });
        sheet_field(
            "Agent",
            None,
            button,
            Some(agent_help(steps, &stage.id).into()),
            PAD,
        )
    }

    /// The title of a folding group: chevron, title, "· N changed"; a click, Enter or Space
    /// opens or folds it.
    fn fold_title(&self, stage: &Stage, group: SheetGroup, cx: &mut Context<Self>) -> AnyElement {
        let open = self.open[group.index()];
        let hover = cx.theme().colors().ghost_element_hover;
        let chevron = Icon::new(if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        })
        .size(IconSize::Small)
        .color(Color::Placeholder);
        let summary = Label::new(group_summary(changed_in(stage, group)))
            .size(LabelSize::Small)
            .color(Color::Placeholder);
        title_row()
            .id(SharedString::from(format!("stage-fold-{}", group.index())))
            .debug_selector(move || format!("stage-fold-{}", group.index()))
            .track_focus(&self.fold_focus[group.index()])
            .rounded(rems_from_px(4.0_f32))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(chevron)
            .child(title_label(group.title()))
            .child(summary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(group, cx)))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    this.toggle_group(group, cx);
                }
            }))
            .into_any_element()
    }

    fn group_rows(
        &self,
        stage: &Stage,
        shows: &SheetShows,
        group: SheetGroup,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let fields = &self.fields;
        let not_used = |unused: bool| unused.then(|| SharedString::from(NOT_USED));
        match group {
            SheetGroup::DoneCheck => vec![
                sheet_field(
                    "Check command",
                    None,
                    line_input(&fields.check, INPUT_H),
                    None,
                    PAD,
                ),
                sheet_field(
                    "Finished when",
                    None,
                    line_input(&fields.done_when, INPUT_H),
                    None,
                    PAD,
                ),
            ],
            SheetGroup::Files => {
                let mut rows = Vec::new();
                if shows.args {
                    let input = line_input(&fields.args, INPUT_H);
                    rows.push(sheet_field(
                        "Arguments",
                        None,
                        input,
                        not_used(shows.args_unused),
                        PAD,
                    ));
                }
                if shows.command && !command_at_left(shows) {
                    let area = SettingsInput::new(&fields.command.editor).area();
                    rows.push(sheet_field(
                        "Command",
                        None,
                        area,
                        not_used(shows.command_unused),
                        PAD,
                    ));
                }
                if shows.output {
                    let input = line_input(&fields.output, INPUT_H);
                    rows.push(sheet_field("Output file", None, input, None, PAD));
                }
                if shows.sections {
                    let file = stage.output.clone().unwrap_or_default();
                    let area = SettingsInput::new(&fields.sections.editor).area();
                    rows.push(sheet_field(
                        "Required sections",
                        None,
                        area,
                        Some(format!("{file} must have each.").into()),
                        PAD,
                    ));
                }
                rows
            }
            SheetGroup::Options => option_rows(stage, shows, &self.pipeline, cx),
        }
    }
}
