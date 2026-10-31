//! A button that shows one icon.

use gpui::{
    AnyView, App, ClickEvent, CursorStyle, DefiniteLength, ElementId, FocusHandle, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, Window, prelude::FluentBuilder,
};
use zed_ui::{
    ButtonSize, ButtonStyle, Clickable, Color, Disableable, FixedWidth, IconName, IconSize,
    Toggleable, VisibleOnHover,
};

use crate::button_like::{ButtonCommon, ButtonLike, SelectableButton};
use crate::icon::Icon;
use crate::spacing::DynamicSpacing;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum IconButtonShape {
    /// As wide and as high as the icon plus a little room on each side.
    Square,
    /// The button's own height and padding (the default).
    Wide,
}

#[derive(IntoElement)]
pub struct IconButton {
    base: ButtonLike,
    shape: IconButtonShape,
    icon: IconName,
    icon_size: IconSize,
    icon_color: Color,
    selected_icon: Option<IconName>,
    selected_icon_color: Option<Color>,
    selected_style: Option<ButtonStyle>,
    alpha: Option<f32>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            base: ButtonLike::new(id),
            shape: IconButtonShape::Wide,
            icon,
            icon_size: IconSize::default(),
            icon_color: Color::Default,
            selected_icon: None,
            selected_icon_color: None,
            selected_style: None,
            alpha: None,
        }
    }

    pub fn shape(mut self, shape: IconButtonShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn icon_color(mut self, icon_color: Color) -> Self {
        self.icon_color = icon_color;
        self
    }

    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = Some(alpha);
        self
    }

    pub fn selected_icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.selected_icon = icon.into();
        self
    }

    pub fn selected_icon_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.selected_icon_color = color.into();
        self
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.aria_label(label);
        self
    }
}

impl Disableable for IconButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.base = self.base.disabled(disabled);
        self
    }
}

impl Toggleable for IconButton {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.base = self.base.toggle_state(selected);
        self
    }
}

impl SelectableButton for IconButton {
    fn selected_style(mut self, style: ButtonStyle) -> Self {
        self.selected_style = Some(style);
        self.base = self.base.selected_style(style);
        self
    }
}

impl Clickable for IconButton {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_click(handler);
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.base = self.base.cursor_style(cursor_style);
        self
    }
}

impl FixedWidth for IconButton {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.base = self.base.width(width);
        self
    }

    fn full_width(mut self) -> Self {
        self.base = self.base.full_width();
        self
    }
}

impl ButtonCommon for IconButton {
    fn id(&self) -> &ElementId {
        self.base.id()
    }

    fn style(mut self, style: ButtonStyle) -> Self {
        self.base = self.base.style(style);
        self
    }

    fn size(mut self, size: ButtonSize) -> Self {
        self.base = self.base.size(size);
        self
    }

    fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.base = self.base.tooltip(tooltip);
        self
    }

    fn tab_index(mut self, tab_index: impl Into<isize>) -> Self {
        self.base = self.base.tab_index(tab_index);
        self
    }

    fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.base = self.base.track_focus(focus_handle);
        self
    }
}

impl VisibleOnHover for IconButton {
    fn visible_on_hover(mut self, group_name: impl Into<SharedString>) -> Self {
        self.base = self.base.visible_on_hover(group_name);
        self
    }
}

/// The side of a square button: the icon plus room on each side. The room is 2 px at the default
/// UI size (none for an indicator); a custom size gets its own size as room.
fn square_side(size: IconSize, window: &Window, cx: &App) -> Pixels {
    let icon = size.rems() * window.rem_size();
    let room = match size {
        IconSize::Indicator => DynamicSpacing::Base00.px(cx),
        IconSize::Custom(custom) => custom.to_pixels(window.rem_size()),
        _ => DynamicSpacing::Base02.px(cx),
    };
    icon + room * 2.
}

impl RenderOnce for IconButton {
    #[allow(refining_impl_trait)]
    fn render(self, window: &mut Window, cx: &mut App) -> ButtonLike {
        let disabled = self.base.disabled;
        let selected = self.base.selected;
        let icon = self.selected_icon.filter(|_| selected).unwrap_or(self.icon);
        let color = if disabled {
            Color::Disabled
        } else if let Some(style) = self.selected_style.filter(|_| selected) {
            match style {
                ButtonStyle::Tinted(tint) => tint.into(),
                _ => Color::Default,
            }
        } else if selected {
            self.selected_icon_color.unwrap_or(Color::Selected)
        } else {
            Color::Custom(self.icon_color.color(cx).opacity(self.alpha.unwrap_or(1.)))
        };
        let shown = Icon::new(icon).size(self.icon_size).color(color);
        let side = match self.shape {
            IconButtonShape::Square => Some(square_side(self.icon_size, window, cx)),
            IconButtonShape::Wide => None,
        };
        self.base
            .when_some(side, |this, side| this.width(side).height(side.into()))
            .child(shown)
    }
}
