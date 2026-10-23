//! `plainText` in `packages/core/src/terminals/terminal.ts`: text with no escape sequences or
//! control characters except tabs and line breaks, with LF line ends. Pasted into an agent, an
//! escape can end the paste early, so its Enter is not a submit.

/// The same result as the two regular expressions of core, applied in the same order.
pub fn plain_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\x1b'
            && let Some(length) = escape_length(&chars[index..])
        {
            index += length;
            continue;
        }
        kept.push(chars[index]);
        index += 1;
    }
    let mut out = String::with_capacity(kept.len());
    let mut chars = kept.chars().peekable();
    while let Some(char) = chars.next() {
        if char == '\r' {
            chars.next_if_eq(&'\n');
            out.push('\n');
        } else if !is_control(char) {
            out.push(char);
        }
    }
    out
}

/// The length of an OSC (ended by BEL or ST), a CSI, or a two-character escape at the start of
/// `chars`, which starts with ESC. None: no such sequence, the ESC is a lone control character.
fn escape_length(chars: &[char]) -> Option<usize> {
    let next = *chars.get(1)?;
    if next == ']' {
        let end = chars[2..].iter().position(|c| *c == '\x07' || *c == '\x1b');
        match end.map(|at| at + 2) {
            Some(at) if chars[at] == '\x07' => return Some(at + 1),
            Some(at) if chars.get(at + 1) == Some(&'\\') => return Some(at + 2),
            _ => {}
        }
    }
    if next == '[' {
        let mut at = 2;
        while chars.get(at).is_some_and(|c| ('0'..='?').contains(c)) {
            at += 1;
        }
        while chars.get(at).is_some_and(|c| (' '..='/').contains(c)) {
            at += 1;
        }
        return chars
            .get(at)
            .filter(|c| ('@'..='~').contains(*c))
            .map(|_| at + 1);
    }
    (('@'..='Z').contains(&next) || ('\\'..='_').contains(&next)).then_some(2)
}

/// `[\x00-\x08\x0b-\x1f\x7f-\x9f﻿]`.
fn is_control(char: char) -> bool {
    matches!(char, '\x00'..='\x08' | '\x0b'..='\x1f' | '\x7f'..='\u{9f}' | '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_escapes_and_control_characters_and_keeps_tabs_and_line_breaks() {
        assert_eq!(
            plain_text("a\x1b[31mred\x1b[0m\tb\r\nc\rd"),
            "ared\tb\nc\nd"
        );
        assert_eq!(plain_text("x\x1b]0;title\x07y\x1b]8;;u\x1b\\z"), "xyz");
        assert_eq!(
            plain_text("end\x1b[201~ of paste\x1bM\u{feff}\u{85}"),
            "end of paste"
        );
        assert_eq!(plain_text("người\x7f dùng"), "người dùng");
    }

    #[test]
    fn a_broken_escape_leaves_its_printable_part() {
        // An OSC with no end falls back to the two-character escape ESC ].
        assert_eq!(plain_text("a\x1b]title"), "atitle");
        // A CSI with no final character: only the ESC goes.
        assert_eq!(plain_text("a\x1b[12"), "a[12");
        assert_eq!(plain_text("a\x1b"), "a");
    }
}
