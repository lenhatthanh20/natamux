//! The left pane of the stage sheet (T-0313 mockup B): the prompt at full height for a stage
//! that takes one (a testing stage's Command over it); for a stage without a prompt (context,
//! review, release, manual) the run fields (Command, Arguments, Check command, Finished when)
//! in one card. Also the labelled field both panes use, and the Tab ring
//! of the fields.

use gpui::{AnyElement, FocusHandle, Focusable, FontWeight};
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use ui::prelude::*;

use super::advanced_rules::SheetGroup;
use super::editor_find::find_stage;
use super::sheet_rules::{NOT_USED, SheetShows, command_at_left, groups_shown, sheet_shows};
use super::stage_fields_view::LiveField;
use super::stage_options::small_help;
use super::stage_sheet::StageSheet;
use crate::components::form::SettingsInput;

/// A labelled field: label 12 (the changed mark at the right end), 6 px, the control, then the
/// 11 px help 5 px under it. Padding `pad` (y, x).
pub(super) fn sheet_field(
    label: &'static str,
    mark: Option<AnyElement>,
    control: impl IntoElement,
    help: Option<SharedString>,
    pad: (f32, f32),
) -> AnyElement {
    let (pad_y, pad_x) = pad;
    v_flex()
        .debug_selector(move || format!("field-{label}"))
        .w_full()
        .px(rems_from_px(pad_x))
        .py(rems_from_px(pad_y))
        .child(
            h_flex()
                .items_center()
                .gap(rems_from_px(6.0_f32))
                .mb(rems_from_px(6.0_f32))
                .child(Label::new(label).size(LabelSize::Small))
                .child(div().flex_1())
                .children(mark),
        )
        .child(control)
        .children(help.map(|help| div().mt(rems_from_px(5.0_f32)).child(small_help(help))))
        .into_any_element()
}

/// A one-line input of the sheet, `height` px high.
pub(super) fn line_input(field: &LiveField, height: f32) -> AnyElement {
    SettingsInput::new(&field.editor)
        .height(height)
        .into_any_element()
}

/// The 1 px line between two fields of a card, 12 px in from the left.
pub(super) fn card_line(cx: &App) -> AnyElement {
    div()
        .h(px(1.))
        .ml(rems_from_px(12.0_f32))
        .bg(cx.theme().colors().border_variant)
        .into_any_element()
}

/// The words at the right of the left pane's "Run" title.
fn no_prompt_line(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Manual => "No prompt: a manual stage waits for you.",
        _ => "No prompt to edit for this kind of stage.",
    }
}

/// A 12 px semibold muted pane title.
pub(super) fn pane_title(text: &'static str) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .weight(FontWeight::SEMIBOLD)
        .color(Color::Muted)
}

impl StageSheet {
    pub(super) fn left_pane(
        &mut self,
        stage: &Stage,
        steps: &[Step],
        alerts: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shows = sheet_shows(stage, steps);
        let pane = v_flex()
            .id("stage-sheet-left")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .p(rems_from_px(16.0_f32))
            .gap(rems_from_px(8.0_f32))
            .children(
                alerts
                    .into_iter()
                    .map(|alert| div().flex_none().child(alert)),
            );
        let body = if shows.prompt {
            let mut body = Vec::new();
            if command_at_left(&shows) {
                let area = SettingsInput::new(&self.fields.command.editor).area();
                body.push(
                    div()
                        .flex_none()
                        .debug_selector(|| "stage-command".into())
                        .child(sheet_field("Command", None, area, None, (0., 0.)))
                        .into_any_element(),
                );
            }
            body.extend(self.render_prompt(stage, cx));
            body
        } else {
            vec![self.run_card(stage, &shows, cx)]
        };
        pane.children(body).into_any_element()
    }

    /// The run fields of a stage without a prompt, in a card that fills the pane.
    fn run_card(&self, stage: &Stage, shows: &SheetShows, cx: &mut Context<Self>) -> AnyElement {
        let fields = &self.fields;
        let pad = (12.0_f32, 14.0_f32);
        let not_used = |unused: bool| unused.then(|| SharedString::from(NOT_USED));
        let mut rows = Vec::new();
        if shows.command {
            let area = SettingsInput::new(&fields.command.editor).area();
            rows.push(sheet_field(
                "Command",
                None,
                area,
                not_used(shows.command_unused),
                pad,
            ));
        }
        if shows.args {
            let input = line_input(&fields.args, 28.);
            rows.push(sheet_field(
                "Arguments",
                None,
                input,
                not_used(shows.args_unused),
                pad,
            ));
        }
        let check = line_input(&fields.check, 28.);
        rows.push(sheet_field("Check command", None, check, None, pad));
        let done = line_input(&fields.done_when, 28.);
        rows.push(sheet_field("Finished when", None, done, None, pad));
        let colors = cx.theme().colors();
        let mut card = v_flex()
            .id("stage-run-card")
            .debug_selector(|| "stage-run-card".into())
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.left_scroll)
            .bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .rounded(rems_from_px(8.0_f32));
        let count = rows.len();
        for (index, row) in rows.into_iter().enumerate() {
            card = card.child(div().flex_none().child(row));
            if index + 1 < count {
                card = card.child(card_line(cx));
            }
        }
        let title = h_flex()
            .h(rems_from_px(28.0_f32))
            .flex_none()
            .items_center()
            .child(pane_title("Run"))
            .child(div().flex_1())
            .child(
                Label::new(no_prompt_line(stage.kind))
                    .size(LabelSize::XSmall)
                    .color(Color::Placeholder),
            );
        v_flex()
            .flex_1()
            .min_h_0()
            .gap(rems_from_px(8.0_f32))
            .child(title)
            .child(card)
            .into_any_element()
    }

    /// The text fields in drawn order, for the Tab ring: the left pane, Name, then for each open
    /// group of the right column its fold header and its fields.
    pub(super) fn field_stops(&self, cx: &App) -> Vec<FocusHandle> {
        let steps = self.pipeline.read(cx).steps();
        let Some(stage) = find_stage(steps, &self.id) else {
            return Vec::new();
        };
        let shows = sheet_shows(stage, steps);
        let fields = &self.fields;
        let handle = |field: &LiveField| field.editor.focus_handle(cx);
        let mut stops = Vec::new();
        if shows.prompt {
            if command_at_left(&shows) {
                stops.push(handle(&fields.command));
            }
            stops.push(handle(&fields.prompt));
        } else {
            if shows.command {
                stops.push(handle(&fields.command));
            }
            if shows.args {
                stops.push(handle(&fields.args));
            }
            stops.extend([handle(&fields.check), handle(&fields.done_when)]);
        }
        stops.push(handle(&fields.name));
        for group in groups_shown(&shows) {
            stops.push(self.fold_focus[group.index()].clone());
            if !self.open[group.index()] {
                continue;
            }
            let wanted = match group {
                SheetGroup::DoneCheck => vec![&fields.check, &fields.done_when],
                SheetGroup::Files => {
                    let mut list = Vec::new();
                    if shows.args {
                        list.push(&fields.args);
                    }
                    if shows.command && !command_at_left(&shows) {
                        list.push(&fields.command);
                    }
                    if shows.output {
                        list.push(&fields.output);
                    }
                    if shows.sections {
                        list.push(&fields.sections);
                    }
                    list
                }
                SheetGroup::Options => Vec::new(),
            };
            stops.extend(wanted.into_iter().map(&handle));
        }
        stops
    }
}
