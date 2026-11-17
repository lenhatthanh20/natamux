//! The setters many widgets share, the three states of a check, and the raised-surface styles.

use gpui::{
    App, BoxShadow, ClickEvent, CursorStyle, DefiniteLength, InteractiveElement, SharedString,
    Styled, Window, black, px,
};
use theme::{ActiveTheme, Appearance};

/// A widget that runs a handler when clicked.
pub trait Clickable {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
    fn cursor_style(self, cursor_style: CursorStyle) -> Self;
}

/// A widget that can be turned off.
pub trait Disableable {
    fn disabled(self, disabled: bool) -> Self;
}

/// A widget that can be drawn as selected.
pub trait Toggleable {
    fn toggle_state(self, selected: bool) -> Self;
}

/// A widget whose width can be set.
pub trait FixedWidth {
    fn width(self, width: impl Into<DefiniteLength>) -> Self;
    fn full_width(self) -> Self;
}

/// A widget that shows only while the pointer is over the group of that name.
pub trait VisibleOnHover {
    fn visible_on_hover(self, group_name: impl Into<SharedString>) -> Self;
}

impl<E: InteractiveElement + Styled> VisibleOnHover for E {
    fn visible_on_hover(self, group_name: impl Into<SharedString>) -> Self {
        self.invisible()
            .group_hover(group_name, |style| style.visible())
    }
}

/// On, off, or partly on (a check that stands for several).
#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ToggleState {
    #[default]
    Unselected,
    Indeterminate,
    Selected,
}

impl ToggleState {
    /// The state a click moves to: anything not on turns on.
    pub fn inverse(&self) -> Self {
        if *self == Self::Selected {
            Self::Unselected
        } else {
            Self::Selected
        }
    }

    pub fn selected(&self) -> bool {
        *self == Self::Selected
    }

    pub fn from_any_and_all(any_checked: bool, all_checked: bool) -> Self {
        match (any_checked, all_checked) {
            (true, true) => Self::Selected,
            (false, false) => Self::Unselected,
            _ => Self::Indeterminate,
        }
    }
}

impl From<bool> for ToggleState {
    fn from(selected: bool) -> Self {
        if selected {
            Self::Selected
        } else {
            Self::Unselected
        }
    }
}

impl From<Option<bool>> for ToggleState {
    fn from(selected: Option<bool>) -> Self {
        selected.map_or(Self::Indeterminate, Self::from)
    }
}

/// A black shadow: offset y, blur, alpha.
fn shadow(y: f32, blur: f32, alpha: f32) -> BoxShadow {
    BoxShadow::new(px(0.), px(y), black().opacity(alpha)).blur_radius(px(blur))
}

fn light(cx: &App) -> bool {
    cx.theme().appearance() == Appearance::Light
}

/// A surface raised above the window: theme background, large corners, a thin border, shadows.
fn raised<E: Styled>(this: E, shadows: Vec<BoxShadow>, cx: &App) -> E {
    let colors = cx.theme().colors();
    this.bg(colors.elevated_surface_background)
        .rounded_lg()
        .border_1()
        .border_color(colors.border_variant)
        .shadow(shadows)
}

/// Raised-surface styles for any styled element.
pub trait StyledExt: Styled + Sized {
    /// A popover or menu above the window.
    fn elevation_2(self, cx: &App) -> Self {
        let edge = if light(cx) { 0.03 } else { 0.06 };
        raised(self, vec![shadow(2., 3., 0.12), shadow(1., 0., edge)], cx)
    }

    /// A dialog above everything else.
    fn elevation_3(self, cx: &App) -> Self {
        let (near, mid, edge) = if light(cx) {
            (0.06, 0.06, 0.04)
        } else {
            (0.12, 0.08, 0.12)
        };
        let shadows = vec![
            shadow(2., 3., near),
            shadow(3., 6., mid),
            shadow(6., 12., 0.04),
            shadow(1., 0., edge),
        ];
        raised(self, shadows, cx)
    }
}

impl<E: Styled> StyledExt for E {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_turns_on_anything_not_on() {
        assert_eq!(ToggleState::Unselected.inverse(), ToggleState::Selected);
        assert_eq!(ToggleState::Indeterminate.inverse(), ToggleState::Selected);
        assert_eq!(ToggleState::Selected.inverse(), ToggleState::Unselected);
        assert_eq!(ToggleState::from(None), ToggleState::Indeterminate);
        assert_eq!(ToggleState::from(Some(true)), ToggleState::Selected);
        assert_eq!(
            ToggleState::from_any_and_all(true, false),
            ToggleState::Indeterminate
        );
    }
}
