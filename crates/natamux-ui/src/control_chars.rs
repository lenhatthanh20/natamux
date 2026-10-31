//! Control characters shown as visible symbols, so a one-line label never breaks its line.

use std::borrow::Cow;

/// `\n` becomes ⏎, DEL becomes ␡, and every other ASCII control becomes its picture in the
/// Control Pictures block (U+2400 + its code). Other text comes back as it is.
pub(crate) fn visible(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| c.is_ascii_control()) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.chars().map(picture).collect())
}

fn picture(c: char) -> char {
    match c {
        '\n' => '\u{23ce}',
        '\u{7f}' => '\u{2421}',
        c if c.is_ascii_control() => char::from_u32(0x2400 + c as u32).unwrap_or(c),
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_controls_as_pictures_and_leaves_other_text() {
        assert!(matches!(visible("main.rs"), Cow::Borrowed("main.rs")));
        assert_eq!(visible("a\nb"), "a\u{23ce}b");
        assert_eq!(visible("a\tb\rc\0"), "a\u{2409}b\u{240d}c\u{2400}");
        assert_eq!(visible("x\u{7f}\u{1b}"), "x\u{2421}\u{241b}");
        assert_eq!(visible("é\n🚀\u{85}"), "é\u{23ce}🚀\u{85}");
    }
}
