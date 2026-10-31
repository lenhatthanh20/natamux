//! The clickable box under every button: height by size, background and border by style, hover,
//! press and keyboard focus, and the click rules the text boxes rely on.

use gpui::{
    AnyElement, AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, FocusHandle,
    Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Toggled, Window, div,
    prelude::FluentBuilder, relative, transparent_black,
};
use theme::ActiveTheme;
use zed_ui::{
    ButtonSize, ButtonStyle, Clickable, Disableable, FixedWidth, TintColor, Toggleable,
    VisibleOnHover,
};

use crate::spacing::{DynamicSpacing, rems_from_px};

/// A button that takes another style while it is selected.
pub trait SelectableButton: Toggleable {
    fn selected_style(self, style: ButtonStyle) -> Self;
}

/// The setters every button takes.
pub trait ButtonCommon: Clickable + Disableable {
    fn id(&self) -> &ElementId;
    fn style(self, style: ButtonStyle) -> Self;
    fn size(self, size: ButtonSize) -> Self;
    fn tooltip(self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self;
    fn tab_index(self, tab_index: impl Into<isize>) -> Self;
    fn track_focus(self, focus_handle: &FocusHandle) -> Self;
}

/// Background and border of a style in one state.
struct Look {
    background: Hsla,
    border: Hsla,
}

fn tint(tint: TintColor, cx: &App) -> Look {
    let status = cx.theme().status();
    let (background, border) = match tint {
        TintColor::Accent => (status.info_background, status.info_border),
        TintColor::Error => (status.error_background, status.error_border),
        TintColor::Warning => (status.warning_background, status.warning_border),
        TintColor::Success => (status.success_background, status.success_border),
    };
    Look { background, border }
}

/// At rest. A disabled button keeps this look too; only its text and cursor change.
fn resting(style: ButtonStyle, cx: &App) -> Look {
    let colors = cx.theme().colors();
    match style {
        ButtonStyle::Filled => Look {
            background: colors.element_background,
            border: transparent_black(),
        },
        ButtonStyle::Tinted(color) => tint(color, cx),
        ButtonStyle::Outlined => Look {
            background: colors.element_background,
            border: colors.border_variant,
        },
        ButtonStyle::OutlinedGhost => Look {
            background: transparent_black(),
            border: colors.border_variant,
        },
        ButtonStyle::OutlinedCustom(border) => Look {
            background: transparent_black(),
            border,
        },
        ButtonStyle::Subtle => Look {
            background: colors.ghost_element_background,
            border: transparent_black(),
        },
        ButtonStyle::Transparent => Look {
            background: transparent_black(),
            border: transparent_black(),
        },
    }
}

/// Under the pointer (and, for the styles with no border, with keyboard focus).
fn hovered_background(style: ButtonStyle, cx: &App) -> Hsla {
    let colors = cx.theme().colors();
    match style {
        ButtonStyle::Filled => {
            let mut background = colors.element_background;
            background.fade_out(0.5);
            background
        }
        ButtonStyle::Tinted(color) => {
            let theme = cx.theme();
            theme.darken(tint(color, cx).background, 0.05, 0.2)
        }
        ButtonStyle::Outlined
        | ButtonStyle::OutlinedGhost
        | ButtonStyle::OutlinedCustom(_)
        | ButtonStyle::Subtle => colors.ghost_element_hover,
        ButtonStyle::Transparent => transparent_black(),
    }
}

/// While the mouse button is held down on it.
fn pressed_background(style: ButtonStyle, cx: &App) -> Hsla {
    let colors = cx.theme().colors();
    match style {
        ButtonStyle::Filled | ButtonStyle::Outlined | ButtonStyle::OutlinedCustom(_) => {
            colors.element_active
        }
        ButtonStyle::Tinted(color) => tint(color, cx).background,
        ButtonStyle::Subtle => colors.ghost_element_active,
        ButtonStyle::OutlinedGhost | ButtonStyle::Transparent => transparent_black(),
    }
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type TooltipBuilder = Box<dyn Fn(&mut Window, &mut App) -> AnyView + 'static>;

/// A box that acts as a button around any children.
#[derive(IntoElement)]
pub struct ButtonLike {
    base: Div,
    id: ElementId,
    style: ButtonStyle,
    pub(crate) disabled: bool,
    pub(crate) selected: bool,
    selected_style: Option<ButtonStyle>,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    size: ButtonSize,
    tab_index: Option<isize>,
    focus_handle: Option<FocusHandle>,
    pub(crate) aria_label: Option<SharedString>,
    pub(crate) aria_keyshortcuts: Option<SharedString>,
    toggled: Option<bool>,
    tooltip: Option<TooltipBuilder>,
    cursor_style: CursorStyle,
    on_click: Option<ClickHandler>,
    children: Vec<AnyElement>,
}

impl ButtonLike {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div(),
            id: id.into(),
            style: ButtonStyle::default(),
            disabled: false,
            selected: false,
            selected_style: None,
            width: None,
            height: None,
            size: ButtonSize::Default,
            tab_index: None,
            focus_handle: None,
            aria_label: None,
            aria_keyshortcuts: None,
            toggled: None,
            tooltip: None,
            cursor_style: CursorStyle::PointingHand,
            on_click: None,
            children: Vec::new(),
        }
    }

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.height = Some(height);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.base = self.base.opacity(opacity);
        self
    }

    /// The name screen readers say for the button.
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }
}

impl Disableable for ButtonLike {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Toggleable for ButtonLike {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self.toggled = Some(selected);
        self
    }
}

impl SelectableButton for ButtonLike {
    fn selected_style(mut self, style: ButtonStyle) -> Self {
        self.selected_style = Some(style);
        self
    }
}

impl Clickable for ButtonLike {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.cursor_style = cursor_style;
        self
    }
}

impl FixedWidth for ButtonLike {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    fn full_width(mut self) -> Self {
        self.width = Some(relative(1.));
        self
    }
}

impl ButtonCommon for ButtonLike {
    fn id(&self) -> &ElementId {
        &self.id
    }

    fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    fn tab_index(mut self, tab_index: impl Into<isize>) -> Self {
        self.tab_index = Some(tab_index.into());
        self
    }

    fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.focus_handle = Some(focus_handle.clone());
        self
    }
}

impl VisibleOnHover for ButtonLike {
    fn visible_on_hover(mut self, group_name: impl Into<SharedString>) -> Self {
        self.base = self.base.visible_on_hover(group_name);
        self
    }
}

impl ParentElement for ButtonLike {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The height of a button size.
fn height_of(size: ButtonSize) -> DefiniteLength {
    let px = match size {
        ButtonSize::Large => 32_f32,
        ButtonSize::Medium => 28_f32,
        ButtonSize::Default => 22_f32,
        ButtonSize::Compact => 18_f32,
        ButtonSize::None => 16_f32,
    };
    rems_from_px(px).into()
}

impl RenderOnce for ButtonLike {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        // The selected style changes the colours; the border follows the button's own style.
        let style = self
            .selected_style
            .filter(|_| self.selected)
            .unwrap_or(self.style);
        let outlined = matches!(
            self.style,
            ButtonStyle::Outlined | ButtonStyle::OutlinedGhost | ButtonStyle::OutlinedCustom(_)
        );
        let rest = resting(style, cx);
        let hover = hovered_background(style, cx);
        let pressed = pressed_background(style, cx);
        let focus_border = cx.theme().colors().border_focused;
        let ui_font = theme::theme_settings(cx).ui_font(cx).family.clone();
        let disabled = self.disabled;
        let cursor = self.cursor_style;

        self.base
            .flex()
            .flex_row()
            .items_center()
            .id(self.id)
            .role(Role::Button)
            .when_some(self.aria_label, |this, label| this.aria_label(label))
            .when_some(self.aria_keyshortcuts, |this, keys| {
                this.aria_keyshortcuts(keys)
            })
            .when_some(self.toggled, |this, toggled| {
                this.aria_toggled(if toggled {
                    Toggled::True
                } else {
                    Toggled::False
                })
            })
            // A focused button stays a tab stop while it is disabled, so the focus stays here.
            .when_some(self.tab_index, |this, index| {
                this.tab_index(index).tab_stop(!disabled)
            })
            .when_some(self.focus_handle, |this, handle| this.track_focus(&handle))
            .font_family(ui_font)
            .group("")
            .flex_none()
            .h(self.height.unwrap_or(height_of(self.size)))
            .when_some(self.width, |this, width| {
                this.w(width).justify_center().text_center()
            })
            .when(outlined, |this| this.border_1())
            .rounded_sm()
            .gap(DynamicSpacing::Base04.rems(cx))
            .map(|this| match self.size {
                ButtonSize::Large | ButtonSize::Medium => this.px(DynamicSpacing::Base08.rems(cx)),
                ButtonSize::Default | ButtonSize::Compact => {
                    this.px(DynamicSpacing::Base04.rems(cx))
                }
                ButtonSize::None => this.px_px(),
            })
            .border_color(rest.border)
            .bg(rest.background)
            .when(disabled, |this| {
                if cursor == CursorStyle::PointingHand {
                    this.cursor_not_allowed()
                } else {
                    this.cursor(cursor)
                }
            })
            .when(!disabled, |this| {
                let hover_style = move |look: StyleRefinement| look.bg(hover);
                this.cursor(cursor)
                    .hover(hover_style)
                    .map(|this| {
                        if outlined {
                            this.focus_visible(move |look| look.border_color(focus_border))
                        } else {
                            this.focus_visible(hover_style)
                        }
                    })
                    .active(move |look| look.bg(pressed))
            })
            .when_some(self.on_click.filter(|_| !disabled), |this, on_click| {
                // A press does not take the focus from a text box; a click goes no further.
                this.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_click(event, window, cx)
                    })
            })
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip(move |window, cx| tooltip(window, cx))
            })
            .children(self.children)
    }
}
