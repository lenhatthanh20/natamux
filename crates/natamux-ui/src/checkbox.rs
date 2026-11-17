//! A check box with an optional label. A click passes the state it moves to.

use gpui::{
    AnyView, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use theme::ActiveTheme;

use crate::color::Color;
use crate::icon::{Icon, IconSize};
use crate::icon_name::IconName;
use crate::label::{Label, LabelCommon, LabelSize};
use crate::spacing::{DynamicSpacing, h_flex};
use crate::traits::ToggleState;

type ClickHandler = Box<dyn Fn(&ToggleState, &ClickEvent, &mut Window, &mut App) + 'static>;
type TooltipBuilder = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// A 16 px box in a 20 px square: a check when on, a dash when partly on.
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    state: ToggleState,
    disabled: bool,
    filled: bool,
    display_only: bool,
    label: Option<SharedString>,
    label_size: LabelSize,
    label_color: Color,
    tooltip: Option<TooltipBuilder>,
    on_click: Option<ClickHandler>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, state: ToggleState) -> Self {
        Self {
            id: id.into(),
            state,
            disabled: false,
            filled: false,
            display_only: false,
            label: None,
            label_size: LabelSize::Default,
            label_color: Color::Muted,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Runs with the state the click moves to.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ToggleState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(move |state, _, window, cx| {
            handler(state, window, cx)
        }));
        self
    }

    /// The element background inside the box instead of the ghost one.
    pub fn fill(mut self) -> Self {
        self.filled = true;
        self
    }

    /// Only shows a state: no hover border and the default cursor.
    pub fn visualization_only(mut self, display_only: bool) -> Self {
        self.display_only = display_only;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    pub fn label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }

    /// The side of the square the box sits in.
    pub fn container_size() -> Pixels {
        px(20.)
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let mark_color = if self.disabled {
            Color::Disabled
        } else {
            Color::Selected
        };
        let mark = match self.state {
            ToggleState::Selected => Some(IconName::Check),
            ToggleState::Indeterminate => Some(IconName::Dash),
            ToggleState::Unselected => None,
        }
        .map(|name| Icon::new(name).size(IconSize::Small).color(mark_color));
        let background = if self.filled {
            colors.element_background
        } else {
            colors.ghost_element_background
        };
        let border = if self.disabled {
            colors.border_variant
        } else {
            colors.border
        };
        let hover_border = border.alpha(0.7);
        let group = SharedString::from(format!("checkbox_group_{:?}", self.id));
        let hover_shows = !self.disabled && !self.display_only;

        let square = h_flex()
            .group(group.clone())
            .id(self.id.clone())
            .size(Self::container_size())
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .justify_center()
                    .items_center()
                    .m_1()
                    .size_4()
                    .rounded_xs()
                    .bg(background)
                    .border_1()
                    .border_color(border)
                    .when(self.disabled, |inner| {
                        inner
                            .cursor_not_allowed()
                            .bg(colors.element_disabled.opacity(0.6))
                    })
                    .when(hover_shows, |inner| {
                        inner.group_hover(group, move |look| look.border_color(hover_border))
                    })
                    .children(mark),
            );

        let state = self.state;
        h_flex()
            .id(self.id)
            .map(|row| {
                if self.disabled {
                    row.cursor_not_allowed()
                } else if self.display_only {
                    row.cursor_default()
                } else {
                    row.cursor_pointer()
                }
            })
            .gap(DynamicSpacing::Base06.rems(cx))
            .child(square)
            .when_some(self.label, |row, label| {
                row.child(
                    Label::new(label)
                        .color(self.label_color)
                        .size(self.label_size),
                )
            })
            .when_some(self.tooltip, |row, tooltip| {
                row.tooltip(move |window, cx| tooltip(window, cx))
            })
            .when_some(self.on_click.filter(|_| !self.disabled), |row, on_click| {
                row.on_click(move |click, window, cx| on_click(&state.inverse(), click, window, cx))
            })
    }
}
