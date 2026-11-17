//! The clickable box under every button: height by size, background and border by style, hover,
//! press and keyboard focus, and the click rules the text boxes rely on.

use gpui::{
    AnyElement, AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, FocusHandle,
    Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Rems, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Toggled, Window, div, px, relative,
    transparent_black,
};
use theme::ActiveTheme;

use crate::spacing::{DynamicSpacing, rems_from_px};
use crate::traits::{Clickable, Disableable, FixedWidth, Toggleable, VisibleOnHover};

/// The colour family of a tinted button.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum TintColor {
    #[default]
    Accent,
    Error,
    Warning,
    Success,
}

/// How a button looks at rest, under the pointer and when pressed.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ButtonStyle {
    Filled,
    Tinted(TintColor),
    Outlined,
    OutlinedGhost,
    OutlinedCustom(Hsla),
    #[default]
    Subtle,
    Transparent,
}

impl ButtonStyle {
    /// The styles drawn with a 1 px frame.
    fn framed(self) -> bool {
        matches!(self, Self::Outlined | Self::OutlinedGhost | Self::OutlinedCustom(_))
    }
}

/// The height of a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    Large,
    Medium,
    #[default]
    Default,
    Compact,
    None,
}

impl ButtonSize {
    /// The box height: 32, 28, 22, 18 or 16 px at UI size 16.
    fn height(self) -> Rems {
        let px = match self {
            Self::Large => 32_f32,
            Self::Medium => 28_f32,
            Self::Default => 22_f32,
            Self::Compact => 18_f32,
            Self::None => 16_f32,
        };
        rems_from_px(px)
    }

    /// The room left and right of the content: 8 px for the tall sizes, 4 px for the short ones,
    /// and a fixed 1 px for `None`.
    fn side_room(self, cx: &App) -> DefiniteLength {
        match self {
            Self::Large | Self::Medium => DynamicSpacing::Base08.rems(cx).into(),
            Self::Default | Self::Compact => DynamicSpacing::Base04.rems(cx).into(),
            Self::None => px(1.).into(),
        }
    }
}

/// A button that takes another style while it is selected.
pub trait SelectableButton: Toggleable {
    fn selected_style(self, style: ButtonStyle) -> Self;
}

/// The setters every button takes.
pub trait ButtonCommon: Clickable + Disableable {
    fn id(&self) -> &ElementId;
    fn size(self, size: ButtonSize) -> Self;
    fn style(self, look: ButtonStyle) -> Self;
    fn track_focus(self, handle: &FocusHandle) -> Self;
    fn tab_index(self, index: impl Into<isize>) -> Self;
    fn tooltip(self, build: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self;
}

/// The colours of one style: its box at rest, and its fill under the pointer and while pressed.
/// A disabled button keeps the rest colours; only its text and cursor change.
struct Palette {
    fill: Hsla,
    edge: Hsla,
    hover: Hsla,
    press: Hsla,
}

fn palette(style: ButtonStyle, cx: &App) -> Palette {
    let c = cx.theme().colors();
    let none = transparent_black();
    let row = |fill, edge, hover, press| Palette {
        fill,
        edge,
        hover,
        press,
    };
    match style {
        ButtonStyle::Subtle => row(
            c.ghost_element_background,
            none,
            c.ghost_element_hover,
            c.ghost_element_active,
        ),
        ButtonStyle::Outlined => row(
            c.element_background,
            c.border_variant,
            c.ghost_element_hover,
            c.element_active,
        ),
        ButtonStyle::OutlinedGhost => row(none, c.border_variant, c.ghost_element_hover, none),
        ButtonStyle::OutlinedCustom(edge) => {
            row(none, edge, c.ghost_element_hover, c.element_active)
        }
        ButtonStyle::Filled => row(
            c.element_background,
            none,
            c.element_background.opacity(0.5),
            c.element_active,
        ),
        ButtonStyle::Tinted(family) => {
            let status = cx.theme().status();
            let (fill, edge) = match family {
                TintColor::Accent => (status.info_background, status.info_border),
                TintColor::Error => (status.error_background, status.error_border),
                TintColor::Warning => (status.warning_background, status.warning_border),
                TintColor::Success => (status.success_background, status.success_border),
            };
            row(fill, edge, cx.theme().darken(fill, 0.05, 0.2), fill)
        }
        ButtonStyle::Transparent => row(none, none, none, none),
    }
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type MakeTooltip = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// Everything a button was told before it is drawn. `Button` and `IconButton` read it to pick
/// their text and icon colours and the names screen readers say.
pub(crate) struct Setup {
    pub(crate) id: ElementId,
    style: ButtonStyle,
    style_when_on: Option<ButtonStyle>,
    size: ButtonSize,
    pub(crate) disabled: bool,
    /// Unset until `toggle_state` is called; screen readers hear a toggle only once it is set.
    toggled: Option<bool>,
    expanded: Option<bool>,
    pub(crate) spoken: Option<SharedString>,
    pub(crate) shortcut: Option<SharedString>,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    tab_index: Option<isize>,
    focus: Option<FocusHandle>,
    cursor: CursorStyle,
    on_click: Option<OnClick>,
    tooltip: Option<MakeTooltip>,
    /// Round corners on the left and on the right; a cell inside a toggle group has square inner
    /// sides.
    rounded: (bool, bool),
}

impl Setup {
    pub(crate) fn is_on(&self) -> bool {
        self.toggled == Some(true)
    }
}

/// A box that acts as a button around any children.
#[derive(IntoElement)]
pub struct ButtonLike {
    pub(crate) setup: Setup,
    /// Takes the opacity and the hover visibility set before the box is drawn.
    outer: Div,
    content: Vec<AnyElement>,
}

impl ButtonLike {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let setup = Setup {
            id: id.into(),
            style: ButtonStyle::default(),
            style_when_on: None,
            size: ButtonSize::default(),
            disabled: false,
            toggled: None,
            expanded: None,
            spoken: None,
            shortcut: None,
            width: None,
            height: None,
            tab_index: None,
            focus: None,
            cursor: CursorStyle::PointingHand,
            on_click: None,
            tooltip: None,
            rounded: (true, true),
        };
        Self {
            setup,
            outer: div(),
            content: Vec::new(),
        }
    }

    fn with(mut self, change: impl FnOnce(&mut Setup)) -> Self {
        change(&mut self.setup);
        self
    }

    pub(crate) fn round_sides(self, left: bool, right: bool) -> Self {
        self.with(|s| s.rounded = (left, right))
    }

    pub fn height(self, height: DefiniteLength) -> Self {
        self.with(|s| s.height = Some(height))
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.outer = self.outer.opacity(opacity);
        self
    }

    /// The name screen readers say for the button.
    pub fn aria_label(self, label: impl Into<SharedString>) -> Self {
        self.with(|s| s.spoken = Some(label.into()))
    }

    /// Whether the part the button opens is open, for screen readers.
    pub fn aria_expanded(self, expanded: bool) -> Self {
        self.with(|s| s.expanded = Some(expanded))
    }
}

impl Disableable for ButtonLike {
    fn disabled(self, disabled: bool) -> Self {
        self.with(|s| s.disabled = disabled)
    }
}

impl Toggleable for ButtonLike {
    fn toggle_state(self, selected: bool) -> Self {
        self.with(|s| s.toggled = Some(selected))
    }
}

impl SelectableButton for ButtonLike {
    fn selected_style(self, style: ButtonStyle) -> Self {
        self.with(|s| s.style_when_on = Some(style))
    }
}

impl Clickable for ButtonLike {
    fn on_click(self, then: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.with(|s| s.on_click = Some(Box::new(then)))
    }

    fn cursor_style(self, cursor_style: CursorStyle) -> Self {
        self.with(|s| s.cursor = cursor_style)
    }
}

impl FixedWidth for ButtonLike {
    fn width(self, width: impl Into<DefiniteLength>) -> Self {
        self.with(|s| s.width = Some(width.into()))
    }

    fn full_width(self) -> Self {
        self.with(|s| s.width = Some(relative(1.)))
    }
}

impl ButtonCommon for ButtonLike {
    fn id(&self) -> &ElementId {
        &self.setup.id
    }

    fn size(self, size: ButtonSize) -> Self {
        self.with(|s| s.size = size)
    }

    fn style(self, look: ButtonStyle) -> Self {
        self.with(|s| s.style = look)
    }

    fn track_focus(self, handle: &FocusHandle) -> Self {
        self.with(|s| s.focus = Some(handle.clone()))
    }

    fn tab_index(self, index: impl Into<isize>) -> Self {
        self.with(|s| s.tab_index = Some(index.into()))
    }

    fn tooltip(self, build: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.with(|s| s.tooltip = Some(Box::new(build)))
    }
}

impl VisibleOnHover for ButtonLike {
    fn visible_on_hover(self, group: impl Into<SharedString>) -> Self {
        Self {
            outer: self.outer.visible_on_hover(group),
            ..self
        }
    }
}

impl ParentElement for ButtonLike {
    fn extend(&mut self, items: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(items)
    }
}

impl RenderOnce for ButtonLike {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            setup,
            outer,
            content,
        } = self;
        // A selected style changes only the colours; the frame follows the button's own style.
        let shown = match setup.style_when_on {
            Some(style) if setup.is_on() => style,
            _ => setup.style,
        };
        let colours = palette(shown, cx);
        let framed = setup.style.framed();
        let font = theme::theme_settings(cx).ui_font(cx).family.clone();

        let mut frame = outer
            .id(setup.id)
            .role(Role::Button)
            .group("")
            .flex()
            .flex_none()
            .flex_row()
            .items_center()
            .font_family(font)
            .h(setup.height.unwrap_or_else(|| setup.size.height().into()))
            .px(setup.size.side_room(cx))
            .gap(DynamicSpacing::Base04.rems(cx))
            .bg(colours.fill)
            .border_color(colours.edge);

        if let Some(spoken) = setup.spoken {
            frame = frame.aria_label(spoken);
        }
        if let Some(shortcut) = setup.shortcut {
            frame = frame.aria_keyshortcuts(shortcut);
        }
        if let Some(open) = setup.expanded {
            frame = frame.aria_expanded(open);
        }
        if let Some(on) = setup.toggled {
            frame = frame.aria_toggled(if on { Toggled::True } else { Toggled::False });
        }
        // Tab skips a disabled button.
        if let Some(index) = setup.tab_index {
            frame = frame.tab_index(index).tab_stop(!setup.disabled);
        }
        if let Some(handle) = &setup.focus {
            frame = frame.track_focus(handle);
        }
        if let Some(width) = setup.width {
            frame = frame.w(width).justify_center().text_center();
        }
        if framed {
            frame = frame.border_1();
        }
        frame = match setup.rounded {
            (true, true) => frame.rounded_sm(),
            (true, false) => frame.rounded_l_sm(),
            (false, true) => frame.rounded_r_sm(),
            (false, false) => frame,
        };

        if setup.disabled {
            // The hand turns into "not allowed"; a cursor the caller chose stays.
            frame = match setup.cursor {
                CursorStyle::PointingHand => frame.cursor_not_allowed(),
                other => frame.cursor(other),
            };
        } else {
            let Palette { hover, press, .. } = colours;
            let focus_edge = cx.theme().colors().border_focused;
            frame = frame
                .cursor(setup.cursor)
                .hover(move |look| look.bg(hover))
                .active(move |look| look.bg(press));
            // Keyboard focus shows on the frame where there is one, else as the hover fill.
            frame = if framed {
                frame.focus_visible(move |look| look.border_color(focus_edge))
            } else {
                frame.focus_visible(move |look| look.bg(hover))
            };
            if let Some(handler) = setup.on_click {
                // A press keeps the focus in a text box; the click goes no further.
                frame = frame
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.prevent_default())
                    .on_click(move |click, w, app| {
                        app.stop_propagation();
                        handler(click, w, app)
                    });
            }
        }
        if let Some(tooltip) = setup.tooltip {
            frame = frame.tooltip(move |window, cx| tooltip(window, cx));
        }
        frame.children(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_give_the_heights_of_the_plan() {
        let px = |size: ButtonSize| size.height().0 * 16.;
        assert_eq!(px(ButtonSize::Large), 32.);
        assert_eq!(px(ButtonSize::Medium), 28.);
        assert_eq!(px(ButtonSize::Default), 22.);
        assert_eq!(px(ButtonSize::Compact), 18.);
        assert_eq!(px(ButtonSize::None), 16.);
    }

    #[test]
    fn only_the_outlined_styles_have_a_frame() {
        assert!(ButtonStyle::Outlined.framed());
        assert!(ButtonStyle::OutlinedGhost.framed());
        assert!(ButtonStyle::OutlinedCustom(transparent_black()).framed());
        assert!(!ButtonStyle::Subtle.framed());
        assert!(!ButtonStyle::Filled.framed());
        assert!(!ButtonStyle::Tinted(TintColor::Accent).framed());
        assert!(!ButtonStyle::Transparent.framed());
    }

    #[test]
    fn a_box_is_on_only_once_toggled_on() {
        assert!(!ButtonLike::new("b").setup.is_on());
        assert!(!ButtonLike::new("b").toggle_state(false).setup.is_on());
        assert!(ButtonLike::new("b").toggle_state(true).setup.is_on());
    }

    #[test]
    fn a_box_starts_subtle_default_and_round() {
        let setup = ButtonLike::new("b").setup;
        assert_eq!(setup.style, ButtonStyle::Subtle);
        assert_eq!(setup.size, ButtonSize::Default);
        assert_eq!(setup.cursor, CursorStyle::PointingHand);
        assert_eq!(setup.rounded, (true, true));
        assert!(!setup.disabled && setup.toggled.is_none());
    }
}
