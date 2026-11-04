//! Mouse reports to a program that asked for them (docs/plan-mit-license.md 6.2.5), in the xterm
//! forms: mode 1000 reports presses and releases, 1002 also drags, 1003 every move; the report is
//! `CSI M` with three bytes (X10, coordinates up to 223), the same with UTF-8 coordinates (mode
//! 1005, up to 2015), or `CSI <` with numbers (SGR, mode 1006). The golden table T-M1 is the
//! reference.

use alacritty_terminal::term::TermMode;
use gpui::{Modifiers, MouseButton};

/// What the mouse did, at a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press(MouseButton),
    Release(MouseButton),
    /// A move, with the button held, if any.
    Move(Option<MouseButton>),
}

/// The report of `action` at `line` (from the top of the screen; history above it is negative)
/// and `column`, or None when the program gets none: no mouse mode, Shift held (Shift selects
/// text instead), a move the mode does not report, a line above the screen, or a cell past what
/// the format can say. The view sends a move only when the cell changed.
pub fn report(
    action: MouseAction,
    line: i32,
    column: usize,
    mods: &Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) || mods.shift {
        return None;
    }
    let line = usize::try_from(line).ok()?;
    let (code, pressed) = match action {
        MouseAction::Press(button) => (button_code(button)?, true),
        MouseAction::Release(button) => (button_code(button)?, false),
        MouseAction::Move(Some(button))
            if mode.intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) =>
        {
            (32 + button_code(button)?, true)
        }
        // No button down: 3 is "no button", plus 32 for a move.
        MouseAction::Move(None) if mode.contains(TermMode::MOUSE_MOTION) => (35, true),
        MouseAction::Move(_) => return None,
    };
    let held = 4 * u8::from(mods.shift) + 8 * u8::from(mods.alt) + 16 * u8::from(mods.control);
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        let code = code + held;
        return Some(format!("\x1b[<{code};{};{}{end}", column + 1, line + 1).into_bytes());
    }
    // The old forms cannot name the button of a release.
    let code = if pressed { code } else { 3 };
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let mut bytes = b"\x1b[M".to_vec();
    bytes.push(32 + code + held);
    push_coordinate(&mut bytes, column, utf8)?;
    push_coordinate(&mut bytes, line, utf8)?;
    Some(bytes)
}

fn button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        MouseButton::Navigate(_) => None,
    }
}

/// A 0-based coordinate as 32 + 1 + it: one byte, or a UTF-8 character in mode 1005.
fn push_coordinate(bytes: &mut Vec<u8>, value: usize, utf8: bool) -> Option<()> {
    let value = value + 33;
    if utf8 {
        let ch = char::from_u32(u32::try_from(value).ok().filter(|v| *v < 0x800)?)?;
        let mut buffer = [0; 4];
        bytes.extend(ch.encode_utf8(&mut buffer).as_bytes());
    } else {
        bytes.push(u8::try_from(value).ok()?);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x10_stops_where_a_byte_ends_and_utf8_where_two_bytes_end() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        let press = MouseAction::Press(MouseButton::Left);
        let none = Modifiers::default();
        assert!(report(press, 0, 222, &none, mode).is_some());
        assert_eq!(report(press, 0, 223, &none, mode), None);
        let utf8 = mode | TermMode::UTF8_MOUSE;
        assert!(report(press, 0, 2014, &none, utf8).is_some());
        assert_eq!(report(press, 0, 2015, &none, utf8), None);
        assert_eq!(report(press, -1, 0, &none, mode), None);
    }
}
