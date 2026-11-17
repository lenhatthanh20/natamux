//! An on/off switch: a pill with a round thumb at the end of its state.

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Toggled, Window, div, prelude::FluentBuilder, px,
};
use theme::{ActiveTheme, Appearance};

use crate::spacing::{DynamicSpacing, h_flex};
use crate::traits::ToggleState;

type ClickHandler = Rc<dyn Fn(&ToggleState, &mut Window, &mut App) + 'static>;

/// A 32 x 20 switch. A click passes the state it moves to.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    state: ToggleState,
    disabled: bool,
    label: Option<SharedString>,
    tab_index: Option<isize>,
    on_click: Option<ClickHandler>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>, state: ToggleState) -> Self {
        Self {
            id: id.into(),
            state,
            disabled: false,
            label: None,
            tab_index: None,
            on_click: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ToggleState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// The name screen readers say. It is not drawn: the app puts its own text next to it.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Makes the switch a tab stop that Space and Enter turn (not while disabled).
    pub fn tab_index(mut self, tab_index: impl Into<isize>) -> Self {
        self.tab_index = Some(tab_index.into());
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors();
        let on = self.state == ToggleState::Selected;
        let text = colors.text;
        let (track, edge) = if on {
            (
                theme.status().info.opacity(0.4),
                colors.text_accent.opacity(0.2),
            )
        } else {
            (colors.element_disabled, colors.border)
        };
        // Light themes need a stronger hover tint to show.
        let tint = if theme.appearance() == Appearance::Light {
            1.5
        } else {
            1.0
        };
        let strength = if on { 0.16 } else { 0.05 };
        let hover_track = track.blend(text.opacity(strength * tint));
        let thumb_opacity = if self.disabled {
            0.2
        } else if on {
            1.0
        } else {
            0.5
        };
        let focus_border = colors.border_focused;
        let group = SharedString::from(format!("switch_group_{:?}", self.id));
        let state = self.state;

        let pill = div()
            .id((self.id.clone(), "switch"))
            .role(Role::Switch)
            .when_some(self.label, |pill, label| pill.aria_label(label))
            .aria_toggled(match state {
                ToggleState::Selected => Toggled::True,
                ToggleState::Indeterminate => Toggled::Mixed,
                ToggleState::Unselected => Toggled::False,
            })
            .p(px(1.))
            .border_2()
            .border_color(colors.border_transparent)
            .rounded_full()
            .when_some(self.tab_index.filter(|_| !self.disabled), |pill, index| {
                pill.tab_index(index)
                    .focus_visible(move |look| look.border_color(focus_border))
                    .when_some(self.on_click.clone(), |pill, on_click| {
                        pill.on_click(move |_, window, cx| on_click(&state.inverse(), window, cx))
                    })
            })
            .child(
                h_flex()
                    .w(DynamicSpacing::Base32.rems(cx))
                    .h(DynamicSpacing::Base20.rems(cx))
                    .group(group.clone())
                    .child(
                        h_flex()
                            .map(|row| {
                                if on {
                                    row.justify_end()
                                } else {
                                    row.justify_start()
                                }
                            })
                            .size_full()
                            .rounded_full()
                            .px(DynamicSpacing::Base02.px(cx))
                            .bg(track)
                            .when(!self.disabled, |row| {
                                row.group_hover(group, move |look| look.bg(hover_track))
                            })
                            .border_1()
                            .border_color(edge)
                            .child(
                                div()
                                    .size(DynamicSpacing::Base12.rems(cx))
                                    .rounded_full()
                                    .bg(text)
                                    .opacity(thumb_opacity),
                            ),
                    ),
            );

        h_flex()
            .id(self.id)
            .cursor_pointer()
            .gap(DynamicSpacing::Base06.rems(cx))
            .child(pill)
            .when_some(self.on_click.filter(|_| !self.disabled), |row, on_click| {
                row.on_click(move |_, window, cx| on_click(&state.inverse(), window, cx))
            })
    }
}
