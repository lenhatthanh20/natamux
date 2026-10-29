//! Gate 1's fixed Vietnamese word list, and how typed words are checked against it.

/// Every tone on every vowel, đ, and words that Telex and VNI often get wrong.
pub const WORDS: &str = include_str!("../../assets/ime-words.txt");

pub fn words() -> Vec<&'static str> {
    WORDS
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .collect()
}

/// The text box half: one word at a time, in order.
pub struct WordCheck {
    words: Vec<&'static str>,
    /// (expected, typed) of each wrong word.
    failed: Vec<(&'static str, String)>,
    done: usize,
}

impl WordCheck {
    pub fn new(words: Vec<&'static str>) -> Self {
        WordCheck {
            words,
            failed: Vec::new(),
            done: 0,
        }
    }

    /// The word to type now; None once all are done.
    pub fn expected(&self) -> Option<&'static str> {
        self.words.get(self.done).copied()
    }

    /// Checks a typed word against the expected one. Returns whether it was right.
    pub fn submit(&mut self, typed: &str) -> bool {
        let Some(expected) = self.expected() else {
            return false;
        };
        self.done += 1;
        let right = typed.trim() == expected;
        if !right {
            self.failed.push((expected, typed.trim().to_string()));
        }
        right
    }

    pub fn progress(&self) -> String {
        let passed = self.done - self.failed.len();
        format!(
            "{} of {} typed, {passed} right, {} wrong",
            self.done,
            self.words.len(),
            self.failed.len()
        )
    }

    pub fn failed(&self) -> &[(&'static str, String)] {
        &self.failed
    }
}

/// One shell word for `text`: single quotes, so a space or a quote in a path cannot split it.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The terminal half: writes the list to a file and compares the typed file with it, in the
/// shell, so no new daemon route is needed.
pub fn terminal_check_command(words: &[&str], typed_file: &str) -> String {
    let quoted: Vec<String> = words.iter().map(|word| format!("'{word}'")).collect();
    format!(
        "printf '%s\\n' {} > {typed_file}.expected; if diff {typed_file}.expected {typed_file}; \
         then echo 'IME check: all {} words right'; else echo 'IME check: the lines above differ'; fi\r",
        quoted.join(" "),
        words.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_has_100_words_with_no_quote_to_break_the_shell() {
        let words = words();
        assert_eq!(words.len(), 100);
        assert!(words.iter().all(|word| !word.contains('\'')));
        assert!(words.contains(&"người"));
        assert!(words.contains(&"Đà Nẵng"));
    }

    #[test]
    fn counts_right_and_wrong_words() {
        let mut check = WordCheck::new(vec!["được", "khuỷu", "thuở"]);
        assert_eq!(check.expected(), Some("được"));
        assert!(check.submit(" được "));
        assert!(!check.submit("khuyủ"));
        assert!(check.submit("thuở"));
        assert_eq!(check.expected(), None);
        assert!(!check.submit("more"));
        assert_eq!(check.progress(), "3 of 3 typed, 2 right, 1 wrong");
        assert_eq!(check.failed(), [("khuỷu", "khuyủ".to_string())]);
    }

    #[test]
    fn a_path_with_a_space_or_a_quote_is_one_shell_word() {
        assert_eq!(shell_quote("/a b/x.txt"), "'/a b/x.txt'");
        assert_eq!(shell_quote("/it's/x"), "'/it'\\''s/x'");
        // `.expected` joins the quoted word, so the compare command keeps working.
        let command = terminal_check_command(&["a"], &shell_quote("/a b/x.txt"));
        assert!(command.contains("> '/a b/x.txt'.expected;"));
        assert!(command.contains("diff '/a b/x.txt'.expected '/a b/x.txt';"));
    }

    #[test]
    fn the_terminal_command_quotes_each_word() {
        let command = terminal_check_command(&["a", "Đà Nẵng"], "/tmp/x.txt");
        assert!(command.starts_with("printf '%s\\n' 'a' 'Đà Nẵng' > /tmp/x.txt.expected;"));
        assert!(command.contains("diff /tmp/x.txt.expected /tmp/x.txt"));
        assert!(command.ends_with("fi\r"));
    }
}
