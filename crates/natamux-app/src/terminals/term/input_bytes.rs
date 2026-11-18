//! What a key press sends to the shell (docs/plan-mit-license.md 6.2.5), in the xterm forms: the
//! cursor and editing keys as `CSI` or `SS3` sequences with the xterm modifier number, Ctrl
//! letters as control bytes, and Alt as an `ESC` before the key (not on macOS, where Option types
//! its own characters). A key this encoder leaves alone goes on to the app, and its text comes
//! through the input handler. The golden table T-K1 is the reference for every case.

use alacritty_terminal::term::TermMode;
use gpui::{Keystroke, Modifiers};

/// The bytes a key sends with the terminal in `mode` (the mode the last frame copied), or None
/// when the key is not the terminal's: the app may take it, or it types its text.
pub fn encode(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let mods = &keystroke.modifiers;
    let plain = !(mods.shift || mods.control || mods.alt || mods.platform);
    let text = |bytes: &str| Some(bytes.as_bytes().to_vec());
    let key = keystroke.key.as_str();
    match key {
        "enter" => match held(mods) {
            Held::None => text("\r"),
            Held::Shift => text("\n"),
            Held::Alt => text("\x1b\r"),
            _ => None,
        },
        "tab" => match held(mods) {
            Held::None => text("\t"),
            Held::Shift => text("\x1b[Z"),
            _ => None,
        },
        "escape" => plain.then(|| b"\x1b".to_vec()),
        "backspace" => match held(mods) {
            Held::None | Held::Shift => text("\x7f"),
            Held::Ctrl => text("\x08"),
            Held::Alt => text("\x1b\x7f"),
            _ => None,
        },
        "space" => (held(mods) == Held::Ctrl).then(|| vec![0]),
        "delete" => plain.then(|| b"\x1b[3~".to_vec()),
        _ => {
            if let Some(last) = cursor_key(key) {
                Some(if !plain {
                    format!("\x1b[1;{}{last}", modifier_number(mods)).into_bytes()
                } else if mode.contains(TermMode::APP_CURSOR) {
                    format!("\x1bO{last}").into_bytes()
                } else {
                    format!("\x1b[{last}").into_bytes()
                })
            } else if let Some(number) = tilde_key(key) {
                Some(if plain {
                    format!("\x1b[{number}~").into_bytes()
                } else {
                    format!("\x1b[{number};{}~", modifier_number(mods)).into_bytes()
                })
            } else if let Some(last) = low_function_key(key) {
                Some(if plain {
                    format!("\x1bO{last}").into_bytes()
                } else {
                    format!("\x1b[1;{}{last}", modifier_number(mods)).into_bytes()
                })
            } else {
                character(key, mods)
            }
        }
    }
}

/// Which modifiers are held, for the keys whose bytes depend on exactly that.
#[derive(PartialEq, Eq)]
enum Held {
    None,
    Shift,
    Ctrl,
    Alt,
    ShiftAlt,
    CtrlAlt,
    Other,
}

fn held(mods: &Modifiers) -> Held {
    match (mods.shift, mods.control, mods.alt, mods.platform) {
        (false, false, false, false) => Held::None,
        (true, false, false, false) => Held::Shift,
        (false, true, false, false) => Held::Ctrl,
        (false, false, true, false) => Held::Alt,
        (true, false, true, false) => Held::ShiftAlt,
        (false, true, true, false) => Held::CtrlAlt,
        _ => Held::Other,
    }
}

/// xterm's modifier number: 1 plus Shift 1, Alt 2, Ctrl 4. Cmd adds nothing.
fn modifier_number(mods: &Modifiers) -> u8 {
    1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.control)
}

/// The arrows, Home and End: the last byte of their sequence.
fn cursor_key(key: &str) -> Option<char> {
    Some(match key {
        "up" => 'A',
        "down" => 'B',
        "right" => 'C',
        "left" => 'D',
        "home" => 'H',
        "end" => 'F',
        _ => return None,
    })
}

/// The keys sent as `CSI number ~`: Insert, the page keys and F5 to F20.
fn tilde_key(key: &str) -> Option<u8> {
    Some(match key {
        "insert" => 2,
        "pageup" => 5,
        "pagedown" => 6,
        "f5" => 15,
        "f6" => 17,
        "f7" => 18,
        "f8" => 19,
        "f9" => 20,
        "f10" => 21,
        "f11" => 23,
        "f12" => 24,
        "f13" => 25,
        "f14" => 26,
        "f15" => 28,
        "f16" => 29,
        "f17" => 31,
        "f18" => 32,
        "f19" => 33,
        "f20" => 34,
        _ => return None,
    })
}

/// F1 to F4: `SS3 P` to `SS3 S`.
fn low_function_key(key: &str) -> Option<char> {
    Some(match key {
        "f1" => 'P',
        "f2" => 'Q',
        "f3" => 'R',
        "f4" => 'S',
        _ => return None,
    })
}

/// A key of one character. Alone or with Shift it types its text; Cmd leaves it to the app.
fn character(key: &str, mods: &Modifiers) -> Option<Vec<u8>> {
    let mut chars = key.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let control = control_byte(ch);
    let alt_is_meta = !cfg!(target_os = "macos");
    let escaped = |c: char| {
        let mut bytes = vec![0x1b];
        bytes.extend(c.to_string().as_bytes());
        bytes
    };
    match held(mods) {
        Held::Ctrl => control.map(|byte| vec![byte]),
        Held::Alt if alt_is_meta => Some(escaped(ch)),
        Held::ShiftAlt if alt_is_meta => Some(escaped(ch.to_ascii_uppercase())),
        Held::CtrlAlt if alt_is_meta && ch.is_ascii_lowercase() => {
            control.map(|byte| vec![0x1b, byte])
        }
        _ => None,
    }
}

/// The control byte of Ctrl with a letter, `[`, `\` or `]`.
fn control_byte(ch: char) -> Option<u8> {
    match ch {
        'a'..='z' => Some(ch as u8 - b'a' + 1),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str, mods: Modifiers, mode: TermMode) -> Option<String> {
        let keystroke = Keystroke {
            modifiers: mods,
            key: key.into(),
            key_char: None,
        };
        encode(&keystroke, mode).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    #[test]
    fn known_values_of_the_plan() {
        let shift = Modifiers::shift();
        assert_eq!(
            press("up", shift, TermMode::empty()).as_deref(),
            Some("\x1b[1;2A")
        );
        assert_eq!(
            press("up", Modifiers::none(), TermMode::APP_CURSOR).as_deref(),
            Some("\x1bOA")
        );
        assert_eq!(
            press("q", Modifiers::control(), TermMode::empty()).as_deref(),
            Some("\x11")
        );
        // A plain letter is the app's: its text comes through the input handler.
        assert_eq!(press("a", Modifiers::none(), TermMode::empty()), None);
    }
}
