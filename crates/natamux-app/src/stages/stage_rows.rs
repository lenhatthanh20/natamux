//! The stage row of the stage card list (layout-b 5.2.1), and the parts the group and branch
//! blocks share: the grip, the "..." button, the caption and the drag and drop of a row.

use gpui::{ClickEvent, Div, DragMoveEvent, FontWeight, KeyDownEvent, MouseButton, Stateful};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::{Stage, StageKind};
use ui::{Switch, ToggleState, Tooltip, prelude::*};

use super::editor_catalog::is_custom;
use super::editor_fields::{has_custom_prompt, set_enabled};
use super::kind_icon::{problem_icon, prompt_icon, waits_icon};
use super::list_rules::{kind_tile, kind_tile_icon, row_meta};
use super::preview_rules::waits_for_person;
use super::source_tag::source_tag;
use super::stage_list::StageList;
use crate::components::button_style::button_icon;
use crate::components::colors::signal;
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::components::tab_order;
use crate::components::text_size::row_text;

/// A row (or a group, or a branch) that is being dragged.
#[derive(Clone)]
pub(super) struct DraggedStage(pub(super) String);

/// What follows the pointer while a row is dragged.
pub(super) struct DragGhost(pub(super) SharedString);

const ROW_HEIGHT: f32 = 40.;

const ROW_TIP: &str = "Click to edit \u{b7} \u{2325}\u{2191} \u{2325}\u{2193} to move";

const CONTEXT_ALWAYS_ON: &str = "The context stage is always on";

/// The problems of one id.
pub(super) fn issues_of(issues: &[PipelineIssue], id: &str) -> Vec<PipelineIssue> {
    issues
        .iter()
        .filter(|issue| issue.stage.as_deref() == Some(id))
        .cloned()
        .collect()
}

/// The drag grip, 14 px: two columns of three dots in the placeholder colour.
pub(super) fn grip(id: &str, cx: &App) -> AnyElement {
    let color = cx.theme().colors().text_placeholder;
    let dot = move || div().size(px(2.)).rounded_full().bg(color);
    let row = move || h_flex().gap(px(2.)).child(dot()).child(dot());
    v_flex()
        .id(SharedString::from(format!("grip-{id}")))
        .flex_none()
        .size(rems_from_px(14.0_f32))
        .gap(px(2.))
        .items_center()
        .justify_center()
        .cursor_grab()
        .tooltip(Tooltip::text("Drag to move"))
        .child(row())
        .child(row())
        .child(row())
        .into_any_element()
}

/// The 24 px "..." button of a row or caption.
pub(super) fn more_button(
    id: &str,
    name: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> IconButton {
    IconButton::new(SharedString::from(format!("more-{id}")), IconName::Ellipsis)
        .icon_size(IconSize::Small)
        .icon_color(Color::Muted)
        .tooltip(Tooltip::text(format!("Actions for {name}")))
        .on_click(on_click)
}

/// Controls inside a clickable row: a press on them is theirs, not the row's.
pub(super) fn own_clicks(controls: impl IntoElement) -> Div {
    h_flex()
        .flex_none()
        .gap(rems_from_px(6.0_f32))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(controls)
}

/// The 28 px caption over the rows of a group or a path: 11 px medium upper case in the
/// placeholder colour, then a 12 px muted note in normal case.
pub(super) fn caption(text: &str, note: Option<String>) -> Div {
    h_flex()
        .h(rems_from_px(28.0_f32))
        .gap(rems_from_px(8.0_f32))
        .overflow_hidden()
        .child(
            Label::new(text.to_uppercase())
                .size(LabelSize::Custom(rems_from_px(11.0_f32)))
                .weight(FontWeight::MEDIUM)
                .color(Color::Placeholder)
                .single_line(),
        )
        .children(note.map(|note| {
            div().min_w_0().child(
                Label::new(note)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            )
        }))
}

/// The box under a caption that holds its rows: a 1 px guide line at the left, 40 px in.
pub(super) fn guide(cx: &App) -> Div {
    v_flex()
        .ml(rems_from_px(40.0_f32))
        .border_l_1()
        .border_color(cx.theme().colors().border)
}

/// The warning or error icon of a row, with the problem as its tooltip.
pub(super) fn problem_mark(id: &str, issues: &[PipelineIssue], cx: &App) -> Option<AnyElement> {
    if issues.is_empty() {
        return None;
    }
    let error = issues.iter().any(|issue| issue.level == IssueLevel::Error);
    let color = if error {
        Color::Error
    } else {
        Color::Custom(signal(cx).fg)
    };
    let tip = issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Some(
        div()
            .id(SharedString::from(format!("problem-{id}")))
            .flex_none()
            .tooltip(Tooltip::text(tip))
            .child(problem_icon().size(IconSize::XSmall).color(color))
            .into_any_element(),
    )
}

fn mark(id: String, icon: Icon, tip: &'static str) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .flex_none()
        .tooltip(Tooltip::text(tip))
        .child(icon.size(IconSize::XSmall).color(Color::Muted))
        .into_any_element()
}

impl StageList {
    /// The accent line where a dragged row would land, next to `id`.
    pub(super) fn drop_line(&self, id: &str, cx: &App) -> Option<impl IntoElement + use<>> {
        let (_, side) = self.target.as_ref().filter(|(target, _)| target == id)?;
        let line = div()
            .absolute()
            .left(px(4.))
            .right(px(4.))
            .h(rems_from_px(2.0_f32))
            .rounded_full()
            .bg(cx.theme().colors().text_accent);
        Some(match side {
            super::editor_moves::Side::Before => line.top(px(-1.)),
            super::editor_moves::Side::After => line.bottom(px(-1.)),
        })
    }

    /// Makes `element` a place to drop a row, and tracks where on it the pointer is.
    pub(super) fn drop_zone<E: InteractiveElement>(
        &self,
        element: E,
        id: &str,
        cx: &mut Context<Self>,
    ) -> E {
        let (over, dropped) = (id.to_owned(), id.to_owned());
        element
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<DraggedStage>, _, cx| {
                    this.drag_over(&over, event, cx);
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedStage, _, cx| {
                this.dropped(&dropped, dragged, cx);
            }))
    }

    pub(super) fn drag_source<E: StatefulInteractiveElement>(
        element: E,
        id: &str,
        label: String,
    ) -> E {
        let (id, label) = (id.to_owned(), SharedString::from(label));
        element.on_drag(DraggedStage(id), move |_, _, _, cx| {
            cx.new(|_| DragGhost(label.clone()))
        })
    }

    /// A 40 px row that opens the sheet of `id` on a click, Enter or Space, and is a tab stop
    /// that Alt+Up and Alt+Down move.
    pub(super) fn row_frame(
        &mut self,
        id: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = cx.theme().colors();
        let (hover, focused_bg) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let handle = self.focus_of(id, cx);
        tab_order::push(&handle);
        let key_id = id.to_owned();
        h_flex()
            .id(SharedString::from(format!("row-{id}")))
            .track_focus(&handle)
            .w_full()
            .h(rems_from_px(ROW_HEIGHT))
            .flex_none()
            .gap(rems_from_px(10.0_f32))
            .pl(rems_from_px(4.0_f32))
            .pr(rems_from_px(8.0_f32))
            .cursor_pointer()
            .when(handle.is_focused(window), |row| row.bg(focused_bg))
            .hover(move |style| style.bg(hover))
            .on_click(self.editor_click(id.to_owned()))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "enter" | "space") && !keys.modifiers.modified() {
                    this.select(&key_id, cx);
                }
            }))
    }

    pub(super) fn stage_row(
        &mut self,
        stage: &Stage,
        issues: &[PipelineIssue],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = stage.id.clone();
        let name = stage_name(stage);
        let on = stage.is_on();
        let mine = issues_of(issues, &id);
        let mut marks = Vec::new();
        if waits_for_person(stage) {
            marks.push(mark(
                format!("waits-{id}"),
                waits_icon(),
                "Waits for approval",
            ));
        }
        if has_custom_prompt(stage) {
            marks.push(mark(format!("prompt-{id}"), prompt_icon(), "Custom prompt"));
        }
        if is_custom(stage) {
            marks.push(source_tag(true, cx));
        }
        marks.extend(problem_mark(&id, &mine, cx));
        let (toggle_id, menu_id) = (id.clone(), id.clone());
        // The context stage opens every pipeline, so it cannot be turned off (T-0333).
        let fixed = stage.kind == StageKind::Context;
        let switch = Switch::new(SharedString::from(format!("on-{id}")), on.into())
            .disabled(fixed)
            .on_click(cx.listener(move |this, _: &ToggleState, _, cx| {
                this.apply(|steps| set_enabled(steps, &toggle_id, !on), None, cx);
            }));
        let switch = div()
            .id(SharedString::from(format!("on-tip-{id}")))
            .when(fixed, |it| it.tooltip(Tooltip::text(CONTEXT_ALWAYS_ON)))
            .child(switch);
        let more = more_button(
            &id,
            &name,
            cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_stage_menu(&menu_id, event.position(), window, cx);
            }),
        );
        let tile = SettingsTile::icon(
            button_icon(kind_tile_icon(stage.kind)),
            kind_tile(stage),
            TileSize::Row,
        );
        let line = self
            .row_frame(&id, window, cx)
            .tooltip(Tooltip::text(ROW_TIP))
            .child(grip(&id, cx))
            .child(tile.dim(!on))
            .child(
                div()
                    .min_w_0()
                    .when(!on, |name| name.opacity(0.5))
                    .child(Label::new(name.clone()).size(row_text()).truncate()),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(rems_from_px(4.0_f32))
                    .children(marks),
            )
            .child(div().flex_1())
            .child(
                Label::new(row_meta(stage))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .single_line(),
            )
            .child(own_clicks(switch).child(more));
        let line = Self::drag_source(line, &id, name);
        let shell = div()
            .id(SharedString::from(format!("item-{id}")))
            .relative()
            .child(line)
            .children(self.drop_line(&id, cx));
        self.drop_zone(shell, &id, cx).into_any_element()
    }

    pub(super) fn editor_click(
        &self,
        id: String,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + use<> {
        let editor = self.editor.clone();
        move |_, _, cx| editor.update(cx, |editor, cx| editor.select(&id, cx))
    }

    pub(super) fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| editor.select(id, cx));
    }
}
