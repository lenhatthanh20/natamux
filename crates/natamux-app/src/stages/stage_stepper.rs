//! The stepper in the stage sheet's header (T-0313 mockup B): "‹ Plan · 3 of 6 · Lint and
//! types ›". It moves the sheet to the stage before or after, in the order of the stage list, so
//! the pipeline can be edited without closing the sheet. Off at the ends.

use gpui::{AnyElement, ClickEvent, FocusHandle};
use natamux_proto::stages::schema::Step;
use ui::{Tooltip, prelude::*};

use super::sheet_rules::{StageLink, stage_neighbours};
use super::stage_sheet::StageSheet;
use crate::components::button_style::quiet_button;

/// A 22 px quiet button to `link`, the chevron before (`back`) or after its name; with no
/// stage there, only the chevron, off.
fn step_button(
    id: &'static str,
    link: Option<StageLink>,
    back: bool,
    focus: &FocusHandle,
    cx: &mut Context<StageSheet>,
) -> Button {
    let icon = Icon::new(if back {
        IconName::ChevronLeft
    } else {
        IconName::ChevronRight
    })
    .size(IconSize::Small)
    .color(Color::Muted);
    let tip = if back { "Previous stage" } else { "Next stage" };
    let label = link
        .as_ref()
        .map_or_else(String::new, |link| link.name.clone());
    let button = quiet_button(id, label)
        .label_size(LabelSize::Small)
        .color(Color::Muted)
        .track_focus(focus)
        .tooltip(Tooltip::text(tip));
    let button = if back {
        button.start_icon(icon)
    } else {
        button.end_icon(icon)
    };
    match link {
        Some(link) => button.on_click(
            cx.listener(move |this, _: &ClickEvent, window, cx| this.go_to(&link.id, window, cx)),
        ),
        None => button.disabled(true),
    }
}

impl StageSheet {
    pub(super) fn stepper(&self, steps: &[Step], cx: &mut Context<Self>) -> AnyElement {
        let Some(at) = stage_neighbours(steps, &self.id) else {
            return div().into_any_element();
        };
        let place = Label::new(format!("{} of {}", at.place, at.count))
            .size(LabelSize::XSmall)
            .color(Color::Placeholder);
        let divider = div()
            .w(px(1.))
            .h(rems_from_px(16.0_f32))
            .mx(rems_from_px(4.0_f32))
            .bg(cx.theme().colors().border_variant);
        h_flex()
            .id("stage-stepper")
            .flex_none()
            .items_center()
            .gap(rems_from_px(2.0_f32))
            .child(step_button(
                "stage-prev",
                at.prev,
                true,
                &self.prev_focus,
                cx,
            ))
            .child(div().px(rems_from_px(4.0_f32)).child(place))
            .child(step_button(
                "stage-next",
                at.next,
                false,
                &self.next_focus,
                cx,
            ))
            .child(divider)
            .into_any_element()
    }

    /// The stepper's buttons that can be pressed, for the Tab ring.
    pub(super) fn stepper_stops(&self, cx: &App) -> Vec<FocusHandle> {
        let Some(at) = stage_neighbours(self.pipeline.read(cx).steps(), &self.id) else {
            return Vec::new();
        };
        let mut stops = Vec::new();
        if at.prev.is_some() {
            stops.push(self.prev_focus.clone());
        }
        if at.next.is_some() {
            stops.push(self.next_focus.clone());
        }
        stops
    }
}
