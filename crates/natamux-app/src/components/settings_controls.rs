//! Small pieces of the Settings screen (layout-b 3.3, 3.11, 5.4): the changed-value mark, the
//! radio, and the labelled field of a sheet.

use gpui::{AnyElement, FontWeight};
use ui::{Tooltip, prelude::*};

use super::colors::accent;
use super::settings_card::{error_line, foot_text};

/// The changed-value mark before a control: a ring at the default, a warning dot when changed,
/// then a reset button that runs `on_reset`. `id` must be unique on the page.
pub fn changed_mark(
    id: impl Into<SharedString>,
    changed: bool,
    on_reset: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let id = id.into();
    let reset_id = SharedString::from(format!("{id}-reset"));
    let ring = if changed {
        cx.theme().status().warning
    } else {
        cx.theme().colors().text_placeholder
    };
    let dot = div()
        .id(SharedString::from(format!("{id}-mark")))
        .flex_none()
        .size(rems_from_px(6.0_f32))
        .rounded_full()
        .border_1()
        .border_color(ring)
        .when(changed, |dot| dot.bg(ring))
        .tooltip(Tooltip::text(if changed { "Changed" } else { "Default" }));
    h_flex()
        .flex_none()
        .gap(rems_from_px(6.0_f32))
        .child(dot)
        .when(changed, |mark| {
            mark.child(
                IconButton::new(reset_id, IconName::RotateCcw)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text("Reset to default"))
                    .on_click(move |_, window, cx| on_reset(window, cx)),
            )
        })
}

/// A 14 px radio: there is none in Zed's ui. The row around it handles the click.
pub fn radio(selected: bool, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let accent = accent(cx);
    let dot = div().size(rems_from_px(6.0_f32)).rounded_full().bg(accent);
    h_flex()
        .flex_none()
        .justify_center()
        .size(rems_from_px(14.0_f32))
        .rounded_full()
        .border_1()
        .border_color(if selected { accent } else { colors.border })
        .bg(colors.editor_background)
        .when(selected, |ring| ring.child(dot))
}

/// A labelled field of a sheet: label, control, then help or the error line.
#[derive(IntoElement)]
pub struct SheetField {
    label: SharedString,
    control: AnyElement,
    help: Option<SharedString>,
    error: Option<SharedString>,
}

impl SheetField {
    pub fn new(label: impl Into<SharedString>, control: impl IntoElement) -> Self {
        Self {
            label: label.into(),
            control: control.into_any_element(),
            help: None,
            error: None,
        }
    }

    pub fn help(mut self, help: Option<impl Into<SharedString>>) -> Self {
        self.help = help.map(Into::into);
        self
    }

    /// Shown instead of the help.
    pub fn error(mut self, error: Option<impl Into<SharedString>>) -> Self {
        self.error = error.map(Into::into);
        self
    }
}

impl RenderOnce for SheetField {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let under = match (self.error, self.help) {
            (Some(error), _) => Some(error_line(error).into_any_element()),
            (None, Some(help)) => Some(
                div()
                    .mt(rems_from_px(3.0_f32))
                    .child(foot_text(help))
                    .into_any_element(),
            ),
            (None, None) => None,
        };
        v_flex()
            .w_full()
            .mb(rems_from_px(12.0_f32))
            .child(
                div().mb(rems_from_px(4.0_f32)).child(
                    Label::new(self.label)
                        .size(LabelSize::Small)
                        .weight(FontWeight::MEDIUM)
                        .color(Color::Muted),
                ),
            )
            .child(self.control)
            .children(under)
    }
}

#[cfg(test)]
#[path = "settings_controls_tests.rs"]
mod tests;
