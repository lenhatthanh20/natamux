//! A segmented control for 2 to 4 exclusive views, such as Table / Board (docs/ui-spec.md 4.6),
//! on Zed's `ToggleButtonGroup`. One tab stop; Left/Right/Up/Down move and pick, wrapping, and
//! Home/End pick the ends. Labels name the option, never the current state.

use std::rc::Rc;

use gpui::{FocusHandle, KeyDownEvent};
use ui::{ToggleButtonGroup, ToggleButtonGroupStyle, ToggleButtonWithIcon, Tooltip, prelude::*};

use super::input_rules::segment_step;

type OnChange = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Segmented<const N: usize> {
    name: SharedString,
    focus: FocusHandle,
    options: [(SharedString, IconName); N],
    selected: usize,
    icon_only: bool,
    on_change: OnChange,
}

impl<const N: usize> Segmented<N> {
    /// `name` names the group (and its element ids); `focus` is the owner's handle made with
    /// `.tab_stop(true)`. `on_change` gets the index of the picked option.
    pub fn new(
        name: impl Into<SharedString>,
        focus: &FocusHandle,
        options: [(SharedString, IconName); N],
        selected: usize,
        on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            focus: focus.clone(),
            options,
            selected,
            icon_only: false,
            on_change: Rc::new(on_change),
        }
    }

    /// Icons only, with the names in tooltips: when main is narrower than 560.
    pub fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }
}

impl<const N: usize> RenderOnce for Segmented<N> {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let selected = self.selected;
        let icon_only = self.icon_only;
        let buttons: [ToggleButtonWithIcon; N] = std::array::from_fn(|index| {
            let (label, icon) = self.options[index].clone();
            let on_click = self.on_change.clone();
            let shown = if icon_only {
                SharedString::default()
            } else {
                label.clone()
            };
            let button = ToggleButtonWithIcon::new(shown, icon, move |_, window, cx| {
                on_click(index, window, cx)
            })
            .selected(index == selected);
            if icon_only {
                button.tooltip(Tooltip::text(label))
            } else {
                button
            }
        });
        let on_key = self.on_change;
        div()
            .id(self.name.clone())
            .flex_none()
            .track_focus(&self.focus)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if let Some(next) = segment_step(N, selected, &event.keystroke.key) {
                    if next != selected {
                        on_key(next, window, cx);
                    }
                    cx.stop_propagation();
                }
            })
            .child(
                ToggleButtonGroup::single_row(self.name, buttons)
                    .style(ToggleButtonGroupStyle::Outlined)
                    .selected_index(selected)
                    .auto_width(),
            )
    }
}
