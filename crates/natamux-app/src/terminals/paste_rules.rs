//! What a paste sends to a terminal: ESC is dropped so the text cannot end the paste early, and
//! the program's bracketed paste marks go around it when it turned them on.

const START: &str = "\x1b[200~";
const END: &str = "\x1b[201~";

pub fn bracketed(text: &str, on: bool) -> String {
    let clean = text.replace('\x1b', "");
    if on {
        format!("{START}{clean}{END}")
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_the_text_when_bracketed_paste_is_on() {
        assert_eq!(bracketed("src/a.ts ", true), "\x1b[200~src/a.ts \x1b[201~");
    }

    #[test]
    fn sends_the_text_as_it_is_when_it_is_off() {
        assert_eq!(bracketed("src/a.ts ", false), "src/a.ts ");
    }

    #[test]
    fn strips_escape_so_the_text_cannot_end_the_paste() {
        assert_eq!(bracketed("a\x1b[201~b", true), "\x1b[200~a[201~b\x1b[201~");
        assert_eq!(bracketed("a\x1bb", false), "ab");
    }
}
