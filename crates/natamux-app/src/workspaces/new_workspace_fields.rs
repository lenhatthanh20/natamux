//! The rows of the Start a task dialog (T-0305, option A "Compact sheet"): one flat column of
//! 32 px rows with short labels. Title; Context from and Skill; the context box (notes or
//! reference, and a bar with the paperclip and the file chips); project and worktree; "from"
//! base "as" branch; agent and flow. Help texts are tooltips. Each select opens its menu
//! (`new_workspace_menus`).

use gpui::{AnyElement, ClickEvent, Div, ExternalPaths, Focusable, KeyDownEvent};
use ui::{ButtonLike, Switch, ToggleState, Tooltip, prelude::*};

use super::super::new_workspace_rules::{SourceChoice, model_error, model_hint, prefill_branch};
use super::super::new_workspace_view_rules::{
    CHOOSE_AGENT, ErrorRow, FLOW_TIP, PICK_AGENT, branch_help, shows_default, source_icon,
    worktree_help, worktree_tip,
};
use super::{DEFAULT_BRANCH_HINT, NewWorkspaceDialog, Pick};
use crate::agents::agent_choices::{agent_label, base_in, custom_agents};
use crate::agents::limits_rules::{current, picker_note};
use crate::app::clock::now_ms;
use crate::components::button_style::button_icon;
use crate::components::colors::accent;
use crate::components::form::SettingsInput;
use crate::components::segmented::Segmented;
use crate::components::settings_card::error_line;
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::components::text_size::row_text;

const ROW_HEIGHT: f32 = 32.;
/// The Branch from select; the branch input takes the rest of its row.
const BASE_WIDTH: f32 = 160.;
/// The agent select; the flow control takes the rest of its row.
const AGENT_WIDTH: f32 = 200.;
const MODEL_TIP: &str = "Passed to the task's agent as --model. Empty: the agent's own default.";

/// The 12 px icon before a select's text.
fn lead_icon(name: &str) -> AnyElement {
    button_icon(name)
        .size(IconSize::XSmall)
        .color(Color::Placeholder)
        .into_any_element()
}

/// A select's text, cut with an ellipsis, and a note after it in the placeholder colour.
fn select_text(text: impl Into<SharedString>, color: Color, note: Option<String>) -> AnyElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap(rems_from_px(4.0_f32))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .child(Label::new(text).size(row_text()).color(color).truncate()),
        )
        .children(note.map(|note| {
            div()
                .flex_none()
                .child(Label::new(note).size(row_text()).color(Color::Placeholder))
        }))
        .into_any_element()
}

/// One 32 px row; its items sit 8 px apart.
fn line() -> Div {
    h_flex()
        .w_full()
        .min_h(rems_from_px(ROW_HEIGHT))
        .gap(rems_from_px(8.0_f32))
}

/// A short muted label before a control.
fn row_label(text: &'static str) -> AnyElement {
    div()
        .flex_none()
        .child(Label::new(text).size(row_text()).color(Color::Muted))
        .into_any_element()
}

/// A label whose tooltip holds the help text that used to sit under the row.
fn tip_label(id: &'static str, text: &'static str, tip: impl Into<SharedString>) -> AnyElement {
    div()
        .id(id)
        .flex_none()
        .tooltip(Tooltip::text(tip.into()))
        .child(Label::new(text).size(row_text()).color(Color::Muted))
        .into_any_element()
}

/// Files dragged over the context box turn its border dashed in the accent colour. The frame
/// lies over the box and takes no click; the drop itself is handled by the whole body.
fn drop_frame(card: impl IntoElement, cx: &App) -> AnyElement {
    let accent = accent(cx);
    div()
        .relative()
        .w_full()
        .child(card)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(rems_from_px(6.0_f32))
                .border_1()
                .border_dashed()
                .border_color(gpui::transparent_black())
                .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(accent)),
        )
        .into_any_element()
}

impl NewWorkspaceDialog {
    /// A select: the lead icon, the picked option and a chevron; a click opens its menu. With no
    /// `width` it fills the rest of its row.
    #[allow(clippy::too_many_arguments)]
    fn select(
        &self,
        id: &'static str,
        pick: Pick,
        lead: AnyElement,
        text: AnyElement,
        width: Option<f32>,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focus = match pick {
            Pick::Source => &self.stops.source,
            Pick::Skill => &self.stops.skill,
            Pick::Project => &self.stops.project,
            Pick::Branch => &self.stops.branch,
            Pick::Agent => &self.stops.agent,
        };
        let button = ButtonLike::new(id)
            .style(ButtonStyle::Outlined)
            .full_width()
            .height(rems_from_px(24.0_f32).into())
            .track_focus(focus)
            .disabled(disabled)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // At the pointer for a mouse click, under the button for Enter or Space.
                this.open_menu(pick, event.position(), window, cx)
            }))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .pl(rems_from_px(8.0_f32))
                    .pr(rems_from_px(6.0_f32))
                    .gap(rems_from_px(6.0_f32))
                    .text_left()
                    .child(lead)
                    .child(text)
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    ),
            );
        div()
            .map(|select| match width {
                Some(width) => select.flex_none().w(rems_from_px(width)),
                None => select.flex_1().min_w_0(),
            })
            .when(disabled, |select| select.opacity(0.45))
            .child(button)
            .into_any_element()
    }

    /// Title, Context from, Skill, the context box and the refused files.
    pub(super) fn task_rows(
        &self,
        choice: &SourceChoice,
        error_row: Option<ErrorRow>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = vec![
            line()
                .child(SettingsInput::new(&self.title))
                .into_any_element(),
        ];
        if self.choices().len() > 1 {
            let lead = lead_icon(source_icon(choice));
            let text = select_text(choice.label(), Color::Default, None);
            let select = self.select("new-task-source", Pick::Source, lead, text, None, false, cx);
            let hint = choice.context_hint(&self.skill);
            let select = div()
                .id("new-task-source-tip")
                .flex_1()
                .min_w_0()
                .tooltip(Tooltip::text(hint.clone()))
                .child(select);
            let label = tip_label("new-task-source-label", "Context from", hint);
            rows.push(line().child(label).child(select).into_any_element());
        }
        if *choice == SourceChoice::AgentSkill {
            let text = if self.skill.is_empty() {
                select_text("Pick a skill", Color::Placeholder, None)
            } else {
                select_text(self.skill.clone(), Color::Default, None)
            };
            let lead = lead_icon("sparkles");
            let select = self.select("new-task-skill", Pick::Skill, lead, text, None, false, cx);
            let error = (error_row == Some(ErrorRow::Skill)).then(|| error_line(PICK_AGENT));
            let row = v_flex()
                .w_full()
                .child(line().child(row_label("Skill")).child(select))
                .children(error);
            rows.push(row.into_any_element());
        }
        rows.push(self.context_box(choice, window, cx));
        rows.extend(
            self.refused
                .iter()
                .map(|reason| error_line(reason.clone()).into_any_element()),
        );
        rows
    }

    /// One bordered box: the notes (or the reference, with its hint as the placeholder), then the
    /// attachments bar.
    fn context_box(
        &self,
        choice: &SourceChoice,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let manual = choice.kind() == "manual";
        let editor = if manual { &self.notes } else { &self.reference };
        let focused = editor.focus_handle(cx).contains_focused(window, cx);
        let colors = cx.theme().colors();
        let border = if focused {
            colors.border_focused
        } else {
            colors.border
        };
        let (bg, bar_line) = (colors.editor_background, colors.border_variant);
        let text = div()
            .w_full()
            .px(rems_from_px(10.0_f32))
            .py(rems_from_px(8.0_f32))
            .child(editor.clone());
        let card = v_flex()
            .w_full()
            .rounded(rems_from_px(6.0_f32))
            .border_1()
            .border_color(border)
            .bg(bg)
            .child(text)
            .child(self.attach_bar(bar_line, cx));
        drop_frame(card, cx)
    }

    /// Project with the worktree switch, then the branch row while the switch is on.
    pub(super) fn workspace_rows(&self, project: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let avatar = SettingsTile::avatar(project, TileSize::Inline).into_any_element();
        let text = select_text(project.to_string(), Color::Default, None);
        let select = self.select(
            "new-task-project",
            Pick::Project,
            avatar,
            text,
            None,
            false,
            cx,
        );
        let label = tip_label(
            "new-task-worktree-label",
            "Worktree",
            worktree_help(self.worktree),
        );
        let mut rows = vec![
            line()
                .child(select)
                .child(label)
                .child(self.worktree_switch(cx))
                .into_any_element(),
        ];
        if self.worktree {
            rows.push(self.branch_row(project, cx));
        }
        rows
    }

    /// The switch in a focus stop of its own: Space or Enter flips it, as a click does.
    fn worktree_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let switch = Switch::new("new-task-worktree-switch", self.worktree.into())
            .on_click(cx.listener(|this, _: &ToggleState, _, cx| this.toggle_worktree(cx)));
        div()
            .id("new-task-worktree")
            .flex_none()
            .track_focus(&self.stops.worktree)
            .tooltip(Tooltip::text(worktree_tip(self.worktree)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    this.toggle_worktree(cx);
                }
            }))
            .child(switch)
            .into_any_element()
    }

    fn toggle_worktree(&mut self, cx: &mut Context<Self>) {
        self.worktree = !self.worktree;
        cx.notify();
    }

    /// "from" the base select "as" the branch input. The select is off, with a tooltip, until
    /// the project's branches are read. Once the user typed a name, a warning dot and a reset
    /// button follow the input.
    fn branch_row(&self, project: &str, cx: &mut Context<Self>) -> AnyElement {
        let loading = self.branches.is_none();
        let note = shows_default(self.branches.as_ref(), &self.base).then(|| "· default".into());
        let text = select_text(self.base_name(), Color::Default, note);
        let lead = lead_icon("git-branch");
        let select = self.select(
            "new-task-branch",
            Pick::Branch,
            lead,
            text,
            Some(BASE_WIDTH),
            loading,
            cx,
        );
        let tip = format!("Reading the branches of {project}\u{2026}");
        let base = div()
            .id("new-task-branch-tip")
            .flex_none()
            .when(loading, |wrap| wrap.tooltip(Tooltip::text(tip)))
            .child(select);
        let title = self.title.read(cx).text(cx);
        let follows = prefill_branch(self.branch_pattern().as_deref(), &title).is_some();
        let help = branch_help(self.branch_edited, follows);
        let mut row = line()
            .child(row_label("from"))
            .child(base)
            .child(tip_label("new-task-branch-as", "as", help))
            .child(
                div()
                    .id("new-task-branch-input")
                    .flex_1()
                    .min_w_0()
                    .tooltip(Tooltip::text(help))
                    .child(SettingsInput::new(&self.branch_name)),
            );
        if self.branch_edited {
            let warning = cx.theme().status().warning;
            let dot = div()
                .id("new-task-branch-mark")
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .border_1()
                .border_color(warning)
                .bg(warning)
                .tooltip(Tooltip::text("Changed"));
            let reset = IconButton::new("new-task-branch-reset", IconName::RotateCcw)
                .size(ButtonSize::Compact)
                .icon_size(IconSize::XSmall)
                .track_focus(&self.stops.reset)
                .tooltip(Tooltip::text("Follow the title again"))
                .on_click(cx.listener(|this, _, _, cx| this.follow_title(cx)));
            row = row.child(dot).child(reset);
        }
        row.into_any_element()
    }

    /// The agent select and the flow control on one row; the flow's red line under it.
    pub(super) fn agent_row(
        &self,
        error_row: Option<ErrorRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let limits = current(&self.dashboard.read(cx).data.limits, now_ms());
        let (icon, text, note) = if self.agent.is_empty() {
            ("terminal", "No agent, a plain terminal".to_string(), None)
        } else {
            let note = picker_note(&self.agent, &limits).map(|note| format!("· {note}"));
            ("bot", agent_label(Some(&self.agent)), note)
        };
        let text = select_text(text, Color::Default, note);
        let select = self.select(
            "new-task-agent",
            Pick::Agent,
            lead_icon(icon),
            text,
            Some(AGENT_WIDTH),
            false,
            cx,
        );
        let this = cx.weak_entity();
        let options = [
            (SharedString::from("Full flow"), IconName::ListTree),
            (SharedString::from("One agent"), IconName::Sparkle),
        ];
        let selected = usize::from(self.single_agent);
        let flow = Segmented::new(
            "new-task-flow",
            &self.stops.flow,
            options,
            selected,
            move |index, _, cx| {
                this.update(cx, |this, cx| {
                    this.single_agent = index == 1;
                    cx.notify();
                })
                .ok();
            },
        );
        let row = line()
            .child(row_label("Agent"))
            .child(select)
            .child(div().flex_1())
            .child(tip_label("new-task-flow-label", "Flow", FLOW_TIP))
            .child(flow);
        let error = (error_row == Some(ErrorRow::Flow)).then(|| error_line(CHOOSE_AGENT));
        v_flex()
            .w_full()
            .child(row)
            .children(error)
            .into_any_element()
    }

    /// The Model input under the agent row, with its red line when the name has bad signs.
    pub(super) fn model_row(&self, cx: &App) -> AnyElement {
        let error = model_error(&self.model.read(cx).text(cx));
        let row = line()
            .child(tip_label("new-task-model-label", "Model", MODEL_TIP))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(SettingsInput::new(&self.model).invalid(error.is_some())),
            );
        v_flex()
            .w_full()
            .child(row)
            .children(error.map(error_line))
            .into_any_element()
    }

    /// Runs on each render, so an agent change gives the Model field its example.
    pub(super) fn sync_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hint = model_hint(base_in(&self.agent, &custom_agents()));
        if hint != self.model_hint {
            self.model_hint = hint.clone();
            self.model.update(cx, |editor, cx| {
                editor.set_placeholder_text(hint.as_str(), window, cx)
            });
        }
    }

    /// Runs on each render, so a title, project or settings change refills the branch with no
    /// event wiring. Once the user edited the field, it stays as typed.
    pub(super) fn sync_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pattern = self.branch_pattern();
        let hint = pattern
            .clone()
            .unwrap_or_else(|| DEFAULT_BRANCH_HINT.into());
        if hint != self.branch_hint {
            self.branch_hint = hint.clone();
            self.branch_name.update(cx, |editor, cx| {
                editor.set_placeholder_text(hint.as_str(), window, cx)
            });
        }
        if self.branch_edited {
            return;
        }
        let title = self.title.read(cx).text(cx);
        let value = prefill_branch(pattern.as_deref(), &title).unwrap_or_default();
        if self.branch_name.read(cx).text(cx) != value {
            self.filled = value.clone();
            self.branch_name
                .update(cx, |editor, cx| editor.set_text(value, window, cx));
        }
    }

    /// Runs on each render, so a source or skill change gives the Reference field its hint.
    pub(super) fn sync_reference(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hint = self.choice().ref_hint(&self.skill);
        if hint != self.ref_hint {
            self.ref_hint = hint.clone();
            self.reference.update(cx, |editor, cx| {
                editor.set_placeholder_text(hint.as_str(), window, cx)
            });
        }
    }
}
