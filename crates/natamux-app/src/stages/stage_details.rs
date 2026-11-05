//! The details pane of the Stages tab (`StageDetails.tsx`): the branch details or the settings of
//! the picked stage, inside the card the page draws. The text fields are editors made when the pick
//! changes and kept otherwise.

use gpui::{AnyElement, ClickEvent, Div, Entity, FocusHandle, FontWeight, Subscription};
use natamux_proto::projects::repo::SkillList;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use ui::{ContextMenu, prelude::*};

use super::advanced_rules::{advanced_has_issue, changed_count};
use super::editor_catalog::is_custom;
use super::editor_fields::{
    TextField, runs_in_own_shell, runs_in_shell_one, set_text, takes_own_agent, takes_prompt,
};
use super::editor_labels::kind_label;
use super::kind_icon::kind_icon;
use super::pipeline_editor::PipelineEditor;
use super::source_tag::{custom_fill, source_tag};
use super::stage_details_fields::StageFields;
use super::stage_fields_view::{
    Choice, SELECT_WIDTH, change, default_mark, details_section, field_area, field_input,
    field_space, issue_list, labelled, labelled_end, open_choices, select,
};
use super::stage_options::{advanced_options, basic_options};
use crate::agents::agent_choices::{agent_label, custom_agents, pick_list};
use crate::components::colors::signal;
use crate::components::tab_order;

/// A command or arguments set in the file on a stage that does not run them stay visible, so a
/// shared file hides nothing.
const NOT_USED: &str = "This stage does not use it. Clear it to remove it from the file.";

pub struct DetailsPane {
    pub(super) editor: Entity<PipelineEditor>,
    pub(super) skills: Option<Result<SkillList, String>>,
    pub(super) stage_fields: Option<StageFields>,
    /// The Advanced block is open; one flag for the page, not saved.
    advanced_open: bool,
    advanced_focus: FocusHandle,
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
            advanced_open: false,
            advanced_focus: cx.focus_handle(),
            skill_menu: None,
            stops: Vec::new(),
            _watch: watch,
        };
        pane.refresh(window, cx);
        pane
    }

    /// The fields on screen, in order.
    pub fn tab_stops(&self, _: &App) -> Vec<FocusHandle> {
        self.stops.clone()
    }

    /// Esc closes the skill menu first.
    pub fn closes_layer(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        self.close_skills(cx)
    }

    /// Makes the fields of the picked stage or branch, and shows the draft in the ones without
    /// the focus.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (steps, stage, issue) = {
            let editor = self.editor.read(cx);
            let stage = editor.selected_stage();
            let id = stage.map(|stage| stage.id.clone());
            let issue = stage
                .is_some_and(|stage| advanced_has_issue(stage, &editor.issues_of(id.as_deref())));
            (editor.steps().to_vec(), id, issue)
        };
        // Kept open after the problem is fixed, so the field being typed in does not vanish.
        if issue {
            self.advanced_open = true;
        }
        self.sync_stage(&steps, stage, window, cx);
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
}

impl Render for DetailsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (steps, branch, stage, issues) = {
            let editor = self.editor.read(cx);
            let id = editor.selected_id();
            (
                editor.steps().to_vec(),
                editor.selected_branch().cloned(),
                editor.selected_stage().cloned(),
                editor.issues_of(id.as_deref()),
            )
        };
        tab_order::begin();
        let body = match (branch, stage) {
            (Some(branch), _) => self.render_branch(&branch, &issues, cx),
            (_, Some(stage)) => self.render_stage(&stage, &steps, &issues, window, cx),
            _ => div()
                .p(rems_from_px(14.0_f32))
                .child(Label::new("Pick a stage").color(Color::Muted))
                .into_any_element(),
        };
        self.stops = tab_order::end();
        v_flex().w_full().child(body)
    }
}

impl DetailsPane {
    fn render_stage(
        &self,
        stage: &Stage,
        steps: &[Step],
        issues: &[PipelineIssue],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(fields) = &self.stage_fields else {
            return div().into_any_element();
        };
        let own_agent = takes_own_agent(steps, &stage.id);
        let mut basic = details_section(Some("Basic"))
            .child(field_space(self.name_field(stage, cx)))
            .children(basic_options(stage, steps, &self.editor, cx));
        if own_agent {
            basic = basic.child(field_space(self.agent_field(stage, steps, cx)));
        }
        if takes_prompt(stage) {
            basic = basic.child(field_space(self.render_prompt(stage, cx)));
        }
        let open = self.advanced_open;
        let mut advanced = v_flex()
            .w_full()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant)
            .child(self.advanced_header(stage, open, window, cx));
        if open {
            advanced = advanced.child(self.advanced_body(stage, steps, own_agent, fields, cx));
        }
        v_flex()
            .w_full()
            .child(stage_header(stage, cx))
            .children(issue_list(issues, cx))
            .child(basic)
            .child(advanced)
            .into_any_element()
    }

    fn toggle_advanced(&mut self, cx: &mut Context<Self>) {
        self.advanced_open = !self.advanced_open;
        cx.notify();
    }

    /// The disclosure row: a chevron, "Advanced", and how many of its values are not the default.
    fn advanced_header(
        &self,
        stage: &Stage,
        open: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let changed = changed_count(stage);
        let (count, count_color) = if changed == 0 {
            ("\u{b7} all default".to_string(), Color::Muted)
        } else {
            (
                format!("\u{b7} {changed} changed"),
                Color::Custom(signal(cx).fg),
            )
        };
        let icon = if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let chevron = IconSize::Custom(rems_from_px(13.0_f32));
        let colors = cx.theme().colors();
        let (focused, hover) = (colors.border_focused, colors.ghost_element_hover);
        tab_order::push(&self.advanced_focus);
        h_flex()
            .id("advanced-toggle")
            .track_focus(&self.advanced_focus)
            .w_full()
            .h(rems_from_px(34.0_f32))
            .px(rems_from_px(14.0_f32))
            .gap_1p5()
            .border_1()
            .border_color(gpui::transparent_black())
            .when(self.advanced_focus.is_focused(window), |this| {
                this.border_color(focused)
            })
            .hover(move |style| style.bg(hover))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.toggle_advanced(cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.toggle_advanced(cx);
                }
            }))
            .child(Icon::new(icon).size(chevron))
            .child(
                Label::new("Advanced")
                    .size(LabelSize::Custom(rems_from_px(13.0_f32)))
                    .weight(FontWeight::MEDIUM),
            )
            .child(Label::new(count).size(LabelSize::Small).color(count_color))
            .into_any_element()
    }

    /// [Arguments | Command], [Check command | Output file], sections, finished when, then the
    /// checkboxes.
    fn advanced_body(
        &self,
        stage: &Stage,
        steps: &[Step],
        own_agent: bool,
        fields: &StageFields,
        cx: &App,
    ) -> AnyElement {
        let args = (own_agent || stage.args.is_some()).then(|| {
            let help = (!own_agent).then(|| SharedString::from(NOT_USED));
            labelled(
                "Arguments",
                None,
                help,
                field_input(&fields.args.editor, cx),
            )
            .into_any_element()
        });
        let [command, check, output, sections, done_when] = self.command_fields(stage, cx);
        let mut body = details_section(None);
        if args.is_some() || command.is_some() {
            body = body.child(two_columns(args, command));
        }
        body.child(two_columns(check, output))
            .children(sections.map(field_space))
            .children(done_when.map(field_space))
            .child(div().mt_0p5())
            .children(advanced_options(stage, steps, &self.editor, cx))
            .into_any_element()
    }

    fn name_field(&self, stage: &Stage, cx: &App) -> AnyElement {
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
        let control = field_input(&fields.name.editor, cx);
        labelled("Name", Some(mark), None, control).into_any_element()
    }

    fn agent_field(&self, stage: &Stage, steps: &[Step], _cx: &App) -> AnyElement {
        let help = if runs_in_shell_one(steps, &stage.id) {
            "\"Task's agent\" shares Terminal 1. Another agent gets a terminal of its own."
        } else if runs_in_own_shell(steps, &stage.id) {
            "This stage runs in a terminal of its own, apart from Terminal 1."
        } else {
            "This stage starts its own agent beside Terminal 1, so it may use another."
        };
        let picked = stage.agent.clone();
        let text = picked
            .as_deref()
            .map_or_else(|| "Task's agent".to_string(), |id| agent_label(Some(id)));
        let (pipeline, id) = (self.editor.clone(), stage.id.clone());
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
        let button = v_flex()
            .w(rems_from_px(SELECT_WIDTH))
            .flex_none()
            .child(button);
        labelled_end("Agent", None, button, Some(help.into()), None).into_any_element()
    }

    /// Command, check, output, sections and finished when; None where the stage has no such field.
    fn command_fields(&self, stage: &Stage, cx: &App) -> [Option<AnyElement>; 5] {
        let Some(fields) = &self.stage_fields else {
            return [None, None, None, None, None];
        };
        let field = |label: &'static str, help: Option<SharedString>, control: Div| {
            Some(labelled(label, None, help, control).into_any_element())
        };
        let testing = stage.kind == StageKind::Testing;
        let agent = stage.kind == StageKind::Agent;
        let command = if testing || stage.command.is_some() {
            let help = (!testing).then(|| SharedString::from(NOT_USED));
            field("Command", help, field_input(&fields.command.editor, cx))
        } else {
            None
        };
        // In the order they are drawn, for the Tab ring.
        let check = field("Check command", None, field_input(&fields.check.editor, cx));
        let output = if agent {
            field("Output file", None, field_input(&fields.output.editor, cx))
        } else {
            None
        };
        let sections = match (agent, &stage.output) {
            (true, Some(file)) => {
                let help = Some(format!("{file} must have each.").into());
                field(
                    "Required sections",
                    help,
                    field_area(&fields.sections.editor, cx),
                )
            }
            _ => None,
        };
        let done_when = field(
            "Finished when",
            None,
            field_input(&fields.done_when.editor, cx),
        );
        [command, check, output, sections, done_when]
    }
}

/// Two fields side by side, 12 px apart, each half the width; a missing one leaves its half empty.
fn two_columns(left: Option<AnyElement>, right: Option<AnyElement>) -> impl IntoElement {
    let half = |field: Option<AnyElement>| div().flex_1().min_w_0().children(field);
    h_flex()
        .w_full()
        .items_start()
        .gap(rems_from_px(12.0_f32))
        .mb(rems_from_px(10.0_f32))
        .child(half(left))
        .child(half(right))
}

/// The 28 px kind box of a details header: the hover colour, or an accent tint for a custom stage.
pub(super) fn kind_box(icon: Icon, accent: bool, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let bg = if accent {
        custom_fill(colors.text_accent)
    } else {
        colors.ghost_element_hover
    };
    h_flex()
        .flex_none()
        .size(rems_from_px(28.0_f32))
        .justify_center()
        .rounded(rems_from_px(7.0_f32))
        .bg(bg)
        .child(icon.size(IconSize::Custom(rems_from_px(15.0_f32))))
}

/// The header of the details card: 10 px 14 px, a border under it.
pub(super) fn details_header(cx: &App) -> Div {
    h_flex()
        .w_full()
        .gap(rems_from_px(10.0_f32))
        .px(rems_from_px(14.0_f32))
        .py(rems_from_px(10.0_f32))
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
}

/// The 14 px semibold name of a details header.
pub(super) fn header_name(name: impl Into<SharedString>) -> Label {
    Label::new(name)
        .size(LabelSize::Custom(rems_from_px(14.0_f32)))
        .weight(FontWeight::SEMIBOLD)
        .truncate()
}

fn stage_header(stage: &Stage, cx: &App) -> impl IntoElement {
    let custom = is_custom(stage);
    let sentence =
        catalog_entry(&stage.id).map_or("A stage of your own", |entry| entry.description);
    let muted = |text: SharedString| Label::new(text).size(LabelSize::Small).color(Color::Muted);
    let color = if custom { Color::Accent } else { Color::Muted };
    let icon = kind_icon(stage.kind).color(color);
    let title = h_flex()
        .min_w_0()
        .gap_1()
        .child(div().min_w_0().child(header_name(stage_name(stage))))
        .child(source_tag(custom, cx));
    let line = h_flex()
        .min_w_0()
        .overflow_hidden()
        .child(muted(format!("{} \u{b7} ", kind_label(stage.kind)).into()))
        .child(muted(stage.id.clone().into()).buffer_font(cx))
        .child(
            div()
                .min_w_0()
                .child(muted(format!(" \u{b7} {sentence}").into()).truncate()),
        );
    details_header(cx)
        .child(kind_box(icon, custom, cx))
        .child(v_flex().flex_1().min_w_0().child(title).child(line))
}
