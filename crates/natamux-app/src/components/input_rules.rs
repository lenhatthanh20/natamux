//! Keyboard rules of the shared parts: the text of a key chip, and the keys of menus, dialogs and
//! segmented controls (docs/ui-spec.md 4.6, 4.11, 4.13, 4.14). No gpui, so it is tested alone.

/// The modifier names a registry key may start with, in gpui form ("mod-shift-p") or in the
/// web's form ("Mod+Shift+P"). "mod" and "secondary" are cmd on macOS and ctrl elsewhere.
const MODIFIERS: [&str; 9] = [
    "mod",
    "secondary",
    "cmd",
    "super",
    "platform",
    "ctrl",
    "control",
    "alt",
    "shift",
];

#[derive(Clone, Copy, Default)]
struct Held {
    ctrl: bool,
    alt: bool,
    shift: bool,
    cmd: bool,
}

/// The text of a key chip for `keys` ("mod-k", "Mod+Shift+P", "shift-f10"; a space separates the
/// strokes of a sequence). macOS joins glyphs ("⇧⌘P"), other systems join words with `+`
/// ("Ctrl+Shift+P").
pub fn kbd_text(keys: &str, mac: bool) -> String {
    keys.split_whitespace()
        .map(|stroke| stroke_text(stroke, mac))
        .collect::<Vec<_>>()
        .join(" ")
}

fn stroke_text(stroke: &str, mac: bool) -> String {
    let mut held = Held::default();
    let mut rest = stroke;
    // Only a known name followed by a separator is a modifier, so "-" in "mod--" stays the key.
    while let Some((name, after)) = split_modifier(rest) {
        match name.as_str() {
            "mod" | "secondary" if mac => held.cmd = true,
            "mod" | "secondary" | "ctrl" | "control" => held.ctrl = true,
            "cmd" | "super" | "platform" => held.cmd = true,
            "alt" => held.alt = true,
            _ => held.shift = true,
        }
        rest = after;
    }
    let key = key_name(rest, mac);
    if mac {
        let mut text = String::new();
        for (on, glyph) in [
            (held.ctrl, "⌃"),
            (held.alt, "⌥"),
            (held.shift, "⇧"),
            (held.cmd, "⌘"),
        ] {
            if on {
                text.push_str(glyph);
            }
        }
        text + &key
    } else {
        let mut parts: Vec<String> = [
            (held.ctrl, "Ctrl"),
            (held.cmd, "Win"),
            (held.alt, "Alt"),
            (held.shift, "Shift"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, word)| word.to_string())
        .collect();
        parts.push(key);
        parts.join("+")
    }
}

fn split_modifier(stroke: &str) -> Option<(String, &str)> {
    let at = stroke.find(['-', '+'])?;
    let name = stroke[..at].to_ascii_lowercase();
    let after = &stroke[at + 1..];
    (MODIFIERS.contains(&name.as_str()) && !after.is_empty()).then_some((name, after))
}

fn key_name(key: &str, mac: bool) -> String {
    let lower = key.to_ascii_lowercase();
    let named = match lower.as_str() {
        "up" | "arrowup" => "↑",
        "down" | "arrowdown" => "↓",
        "left" | "arrowleft" => "←",
        "right" | "arrowright" => "→",
        "escape" | "esc" => "Esc",
        "enter" | "return" if mac => "↵",
        "enter" | "return" => "Enter",
        "backspace" if mac => "⌫",
        "backspace" => "Backspace",
        "delete" if mac => "⌦",
        "delete" => "Delete",
        "tab" => "Tab",
        "space" => "Space",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PgUp",
        "pagedown" => "PgDn",
        "menu" | "contextmenu" => "Menu",
        _ => "",
    };
    if !named.is_empty() {
        named.to_string()
    } else if lower.len() > 1
        && lower.starts_with('f')
        && lower[1..].bytes().all(|b| b.is_ascii_digit())
    {
        lower.to_ascii_uppercase()
    } else if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        key.to_string()
    }
}

/// The modifiers of a key press, as plain flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Press<'a> {
    /// gpui's key name: "escape", "enter", "tab", "up", "f10", "a".
    pub key: &'a str,
    pub shift: bool,
    /// cmd on macOS, ctrl elsewhere.
    pub secondary: bool,
    /// Any other modifier (ctrl on macOS, alt, fn).
    pub other: bool,
}

/// The gpui action name of Zed's `menu` crate for a key in an open context menu (4.13). Tab
/// closes the menu, as Esc does. Keys with modifiers stay with the app (Mod+Q still quits).
pub fn menu_action(press: Press) -> Option<&'static str> {
    if press.secondary || press.other {
        return None;
    }
    Some(match press.key {
        "up" => "menu::SelectPrevious",
        "down" => "menu::SelectNext",
        "home" | "pageup" => "menu::SelectFirst",
        "end" | "pagedown" => "menu::SelectLast",
        "enter" | "space" => "menu::Confirm",
        "escape" | "tab" => "menu::Cancel",
        "right" => "menu::SelectChild",
        "left" => "menu::SelectParent",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKey {
    Cancel,
    Submit,
    Next,
    Previous,
}

/// What a key does in an open dialog (4.14): Esc closes, Mod+Enter submits from any field, Tab and
/// Shift+Tab move inside the dialog only.
pub fn dialog_key(press: Press) -> Option<DialogKey> {
    match press.key {
        "escape" if !press.secondary && !press.other => Some(DialogKey::Cancel),
        "enter" if press.secondary && !press.other => Some(DialogKey::Submit),
        "tab" if !press.secondary && !press.other => Some(if press.shift {
            DialogKey::Previous
        } else {
            DialogKey::Next
        }),
        _ => None,
    }
}

/// The next place in a ring of `len` stops from `current`, wrapping. With no current stop, Tab
/// starts at the first and Shift+Tab at the last. None when the ring is empty.
pub fn ring_step(len: usize, current: Option<usize>, back: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (current, back) {
        (None, false) => 0,
        (None, true) => len - 1,
        (Some(at), false) => (at + 1) % len,
        (Some(at), true) => (at + len - 1) % len,
    })
}

/// The option a key picks in a segmented control of `len` options (4.6): Right and Down move on,
/// Left and Up move back, both wrapping; Home and End pick the ends.
pub fn segment_step(len: usize, selected: usize, key: &str) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match key {
        "right" | "down" => ring_step(len, Some(selected.min(len - 1)), false),
        "left" | "up" => ring_step(len, Some(selected.min(len - 1)), true),
        "home" => Some(0),
        "end" => Some(len - 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str) -> Press<'_> {
        Press {
            key,
            ..Press::default()
        }
    }

    #[test]
    fn shows_mac_keys_as_joined_glyphs() {
        assert_eq!(kbd_text("mod-k", true), "⌘K");
        assert_eq!(kbd_text("Mod+Shift+P", true), "⇧⌘P");
        assert_eq!(kbd_text("ctrl-alt-shift-cmd-x", true), "⌃⌥⇧⌘X");
        assert_eq!(kbd_text("mod-enter", true), "⌘↵");
        assert_eq!(kbd_text("shift-f10", true), "⇧F10");
    }

    #[test]
    fn shows_other_keys_as_words_with_plus() {
        assert_eq!(kbd_text("mod-k", false), "Ctrl+K");
        assert_eq!(kbd_text("Mod+Shift+P", false), "Ctrl+Shift+P");
        assert_eq!(kbd_text("secondary-enter", false), "Ctrl+Enter");
        assert_eq!(kbd_text("alt-1", false), "Alt+1");
    }

    #[test]
    fn names_special_keys_and_keeps_a_dash_key() {
        assert_eq!(kbd_text("escape", false), "Esc");
        assert_eq!(kbd_text("ArrowUp", true), "↑");
        assert_eq!(kbd_text("delete", true), "⌦");
        assert_eq!(kbd_text("mod--", true), "⌘-");
        assert_eq!(kbd_text("/", false), "/");
        assert_eq!(kbd_text("?", true), "?");
        assert_eq!(kbd_text("f", false), "F");
    }

    #[test]
    fn joins_the_strokes_of_a_sequence_with_a_space() {
        assert_eq!(kbd_text("g g", false), "G G");
        assert_eq!(kbd_text("mod-k mod-s", true), "⌘K ⌘S");
    }

    #[test]
    fn maps_menu_keys_to_zed_menu_actions() {
        assert_eq!(menu_action(press("down")), Some("menu::SelectNext"));
        assert_eq!(menu_action(press("up")), Some("menu::SelectPrevious"));
        assert_eq!(menu_action(press("home")), Some("menu::SelectFirst"));
        assert_eq!(menu_action(press("end")), Some("menu::SelectLast"));
        assert_eq!(menu_action(press("enter")), Some("menu::Confirm"));
        assert_eq!(menu_action(press("space")), Some("menu::Confirm"));
        assert_eq!(menu_action(press("escape")), Some("menu::Cancel"));
        assert_eq!(menu_action(press("tab")), Some("menu::Cancel"));
        assert_eq!(menu_action(press("right")), Some("menu::SelectChild"));
        assert_eq!(menu_action(press("left")), Some("menu::SelectParent"));
        assert_eq!(menu_action(press("a")), None);
    }

    #[test]
    fn leaves_menu_keys_with_a_modifier_to_the_app() {
        let quit = Press {
            secondary: true,
            ..press("q")
        };
        assert_eq!(menu_action(quit), None);
        let other = Press {
            other: true,
            ..press("down")
        };
        assert_eq!(menu_action(other), None);
    }

    #[test]
    fn closes_a_dialog_on_esc_and_submits_on_mod_enter_only() {
        assert_eq!(dialog_key(press("escape")), Some(DialogKey::Cancel));
        assert_eq!(dialog_key(press("enter")), None);
        let submit = Press {
            secondary: true,
            ..press("enter")
        };
        assert_eq!(dialog_key(submit), Some(DialogKey::Submit));
        let shift_enter = Press {
            shift: true,
            ..press("enter")
        };
        assert_eq!(dialog_key(shift_enter), None);
    }

    #[test]
    fn moves_tab_and_shift_tab_inside_the_dialog() {
        assert_eq!(dialog_key(press("tab")), Some(DialogKey::Next));
        let back = Press {
            shift: true,
            ..press("tab")
        };
        assert_eq!(dialog_key(back), Some(DialogKey::Previous));
        let ctrl_tab = Press {
            other: true,
            ..press("tab")
        };
        assert_eq!(dialog_key(ctrl_tab), None);
    }

    #[test]
    fn wraps_focus_around_the_ring() {
        assert_eq!(ring_step(3, Some(2), false), Some(0));
        assert_eq!(ring_step(3, Some(0), true), Some(2));
        assert_eq!(ring_step(3, Some(1), false), Some(2));
        assert_eq!(ring_step(3, None, false), Some(0));
        assert_eq!(ring_step(3, None, true), Some(2));
        assert_eq!(ring_step(0, None, false), None);
    }

    #[test]
    fn moves_and_picks_with_arrows_home_and_end() {
        assert_eq!(segment_step(2, 0, "right"), Some(1));
        assert_eq!(segment_step(2, 1, "right"), Some(0));
        assert_eq!(segment_step(3, 0, "left"), Some(2));
        assert_eq!(segment_step(3, 1, "up"), Some(0));
        assert_eq!(segment_step(3, 1, "down"), Some(2));
        assert_eq!(segment_step(4, 2, "home"), Some(0));
        assert_eq!(segment_step(4, 0, "end"), Some(3));
        assert_eq!(segment_step(4, 0, "enter"), None);
        assert_eq!(segment_step(0, 0, "right"), None);
    }
}
