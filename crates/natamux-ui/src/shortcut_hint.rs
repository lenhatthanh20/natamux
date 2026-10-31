//! The key hint of a button (plan D4): the keys of a command in this system's form. On macOS the
//! modifiers and the named keys are icons (⌘ ⌥ ⇧ ⌃ ↵ ⎋ ...) and other keys are text; elsewhere a
//! stroke is one text, such as `Ctrl+Shift+K`. Measured from the shots and the kit metrics golden.

use std::rc::Rc;

use gpui::{
    AbsoluteLength, AnyElement, App, IntoElement, KeybindingKeystroke, Modifiers, ParentElement,
    RenderOnce, SharedString, Styled, Window, px, relative,
};
use zed_ui::{Color, IconName};

use crate::icon::Icon;
use crate::spacing::{h_flex, rems_from_px};

/// The keys of a command, drawn in muted text (disabled text when the command is off).
#[derive(Clone, IntoElement)]
pub struct KeyBinding {
    strokes: Rc<[KeybindingKeystroke]>,
    size: AbsoluteLength,
    disabled: bool,
}

impl KeyBinding {
    pub fn from_keystrokes(strokes: Rc<[KeybindingKeystroke]>) -> Self {
        Self {
            strokes,
            size: rems_from_px(14_f32).into(),
            disabled: false,
        }
    }

    /// The text size, which is also the size of the key icons.
    pub fn size(mut self, size: impl Into<AbsoluteLength>) -> Self {
        self.size = size.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The keys as text, for screen readers: `Cmd+Enter`, `Ctrl+Shift+K`.
    pub fn text(&self) -> SharedString {
        let strokes: Vec<String> = self
            .strokes
            .iter()
            .map(|stroke| {
                let mut words = modifier_words(stroke.modifiers(), MAC);
                words.push(key_word(stroke.key()));
                words.join("+")
            })
            .collect();
        strokes.join(" ").into()
    }
}

const MAC: bool = cfg!(target_os = "macos");

/// The modifier names of a stroke, in the order of the shots and the metrics golden: ⌥ before ⌘,
/// ⌘ before ⇧ (Control first and Fn last are not in any shot). The same order on every system.
fn modifier_words(modifiers: &Modifiers, mac: bool) -> Vec<String> {
    let names: [(bool, &str, &str); 5] = [
        (modifiers.control, "Ctrl", "Ctrl"),
        (modifiers.alt, "Option", "Alt"),
        (modifiers.platform, "Cmd", "Super"),
        (modifiers.shift, "Shift", "Shift"),
        (modifiers.function, "Fn", "Fn"),
    ];
    names
        .into_iter()
        .filter(|(on, _, _)| *on)
        .map(|(_, mac_name, name)| (if mac { mac_name } else { name }).to_string())
        .collect()
}

/// A key as text: a letter in upper case, a named key with a capital (`Enter`, `Escape`).
fn key_word(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The macOS icons of the modifiers, in the order of `modifier_words`.
fn modifier_icons(modifiers: &Modifiers) -> Vec<IconName> {
    [
        (modifiers.control, IconName::Control),
        (modifiers.alt, IconName::Option),
        (modifiers.platform, IconName::Command),
        (modifiers.shift, IconName::Shift),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, icon)| icon)
    .collect()
}

/// The macOS icon of a named key, if it has one.
fn key_icon(key: &str) -> Option<IconName> {
    Some(match key {
        "enter" => IconName::Return,
        "escape" => IconName::Escape,
        "backspace" => IconName::Backspace,
        "delete" => IconName::Delete,
        "tab" => IconName::Tab,
        "space" => IconName::Space,
        "up" => IconName::ArrowUp,
        "down" => IconName::ArrowDown,
        "left" => IconName::ArrowLeft,
        "right" => IconName::ArrowRight,
        "pageup" => IconName::PageUp,
        "pagedown" => IconName::PageDown,
        _ => return None,
    })
}

/// Text of a hint: 2 px of room on each side, one line exactly as high as the text, and at least
/// as wide as an icon key, with the text in the middle.
fn text_part(text: String, size: AbsoluteLength) -> AnyElement {
    h_flex()
        .justify_center()
        .min_w(size)
        .px(px(2.))
        .text_size(size)
        .line_height(relative(1.))
        .child(text)
        .into_any_element()
}

fn icon_part(icon: IconName, size: AbsoluteLength, color: Color) -> AnyElement {
    Icon::new(icon).length(size).color(color).into_any_element()
}

/// One stroke: a row 2 px taller than the text at the top and the bottom.
fn stroke(one: &KeybindingKeystroke, size: AbsoluteLength, color: Color, cx: &App) -> AnyElement {
    let mut parts = Vec::new();
    if MAC {
        for icon in modifier_icons(one.modifiers()) {
            parts.push(icon_part(icon, size, color));
        }
        if one.modifiers().function {
            parts.push(text_part("Fn".to_string(), size));
        }
        parts.push(match key_icon(one.key()) {
            Some(icon) => icon_part(icon, size, color),
            None => text_part(key_word(one.key()), size),
        });
    } else {
        let mut words = modifier_words(one.modifiers(), false);
        words.push(key_word(one.key()));
        parts.push(text_part(words.join("+"), size));
    }
    h_flex()
        .flex_none()
        .py(px(2.))
        .text_color(color.color(cx))
        .children(parts)
        .into_any_element()
}

impl RenderOnce for KeyBinding {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = if self.disabled {
            Color::Disabled
        } else {
            Color::Muted
        };
        h_flex().flex_none().gap(px(4.)).children(
            self.strokes
                .iter()
                .map(|one| stroke(one, self.size, color, cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(control: bool, alt: bool, shift: bool, platform: bool) -> Modifiers {
        Modifiers {
            control,
            alt,
            shift,
            platform,
            ..Modifiers::default()
        }
    }

    #[test]
    fn writes_the_modifiers_in_this_systems_order() {
        let all = mods(true, true, true, true);
        assert_eq!(
            modifier_words(&all, false),
            ["Ctrl", "Alt", "Super", "Shift"]
        );
        assert_eq!(
            modifier_words(&all, true),
            ["Ctrl", "Option", "Cmd", "Shift"]
        );
        assert_eq!(
            modifier_icons(&mods(false, true, false, true)),
            [IconName::Option, IconName::Command]
        );
        assert_eq!(
            modifier_icons(&mods(false, false, true, true)),
            [IconName::Command, IconName::Shift]
        );
        assert!(modifier_words(&mods(false, false, false, false), false).is_empty());
    }

    #[test]
    fn names_a_key_with_a_capital() {
        assert_eq!(key_word("a"), "A");
        assert_eq!(key_word("enter"), "Enter");
        assert_eq!(key_word("escape"), "Escape");
        assert_eq!(key_word(""), "");
        assert_eq!(key_icon("enter"), Some(IconName::Return));
        assert_eq!(key_icon("k"), None);
    }
}
