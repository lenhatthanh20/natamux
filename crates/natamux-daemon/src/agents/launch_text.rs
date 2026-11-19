//! Reading the output of an agent's terminal: what the shell says when it does not know the
//! agent, the mark of an agent that ended with an error, the agent's own last line, and the bell.

use natamux_core::js::{js_trim, utf16_len};

use super::runner::AGENT_FAILED_TITLE;

/// What shells say when they do not know a command: bash, zsh, sh and dash, fish, PowerShell, cmd.
const NOT_FOUND: [&str; 4] = [
    "command not found",
    ": not found",
    "unknown command",
    "is not recognized",
];

/// A line quoted in a reason is cut to this many characters: a full-screen agent may glue its
/// rows into one line.
const QUOTE_CHARS: usize = 200;

/// The text without colors and cursor moves, which would split the words of a line:
/// `ESC [ params letter`, and `ESC ] ... (BEL | ESC \)`. A cursor move becomes a space: full-screen
/// agents (Claude Code, Codex) put the words of a line apart with moves, not spaces.
pub fn without_escapes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\x1b'
            && let Some(end) = escape_end(&chars, at)
        {
            let csi = chars.get(at + 1) == Some(&'[');
            if csi && matches!(chars[end - 1], 'A'..='H' | 'f' | 'd') {
                out.push(' ');
            }
            at = end;
            continue;
        }
        out.push(chars[at]);
        at += 1;
    }
    out
}

/// Where the escape sequence that starts at `start` ends, if one does.
fn escape_end(chars: &[char], start: usize) -> Option<usize> {
    match *chars.get(start + 1)? {
        '[' => {
            let mut at = start + 2;
            while chars
                .get(at)
                .is_some_and(|c| c.is_ascii_digit() || matches!(*c, ';' | '?'))
            {
                at += 1;
            }
            chars
                .get(at)
                .filter(|c| c.is_ascii_alphabetic())
                .map(|_| at + 1)
        }
        ']' => {
            let mut at = start + 2;
            while let Some(c) = chars.get(at) {
                match *c {
                    '\x07' => return Some(at + 1),
                    '\x1b' => {
                        return (chars.get(at + 1) == Some(&'\\')).then_some(at + 2);
                    }
                    _ => at += 1,
                }
            }
            None
        }
        _ => None,
    }
}

/// The shell's line that says it could not find the program, if the text holds one. `plain`:
/// the text `without_escapes`, which the caller has for its other readers too.
pub fn not_found_in(plain: &str, program: &str) -> Option<String> {
    let name = program.rsplit(['\\', '/']).next().unwrap_or(program);
    plain
        .split(['\r', '\n'])
        .map(js_trim)
        .find(|line| {
            // The name first: most lines do not hold it, and the words cost more to look for.
            line.contains(name) && NOT_FOUND.iter().any(|words| holds_lower(line, words))
        })
        .map(String::from)
}

/// `line.to_lowercase().contains(words)` for `words` in lower-case ASCII, with no copy of a line
/// that is ASCII, as nearly every line of a shell is.
fn holds_lower(line: &str, words: &str) -> bool {
    if !line.is_ascii() {
        return line.to_lowercase().contains(words);
    }
    let mut parts = line.as_bytes().windows(words.len());
    parts.any(|part| part.eq_ignore_ascii_case(words.as_bytes()))
}

/// Where the title that the launch line sets on an error starts. The typed line on Windows holds
/// the title's text too, but not the escape before it.
pub fn failed_mark_at(text: &str) -> Option<usize> {
    // Looked for in place, with no pattern built: this runs for every chunk.
    let is_mark = |title: &str| {
        let name = title.strip_prefix("0;");
        let name = name.or_else(|| title.strip_prefix("2;"));
        name.is_some_and(|name| name.starts_with(AGENT_FAILED_TITLE))
    };
    let mut starts = text.match_indices("\x1b]").map(|(at, _)| at);
    starts.find(|at| is_mark(&text[at + 2..]))
}

const BELL: u8 = 0x07;
const ESC: u8 = 0x1b;

/// Where a stream of output stands in an OSC sequence (`ESC ] ... BEL`, such as a window title).
/// Its BEL ends the sequence and rings nothing. Kept across chunks: a sequence can come in two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BellScan {
    #[default]
    Text,
    Escape,
    Osc,
    OscEscape,
}

impl BellScan {
    /// Whether the chunk rings the terminal bell outside OSC sequences.
    pub fn rings(&mut self, data: &[u8]) -> bool {
        if *self == BellScan::Text && !data.contains(&ESC) {
            return data.contains(&BELL);
        }
        let mut rang = false;
        for &byte in data {
            *self = match (*self, byte) {
                (BellScan::Osc, BELL) => BellScan::Text,
                (BellScan::Osc, ESC) => BellScan::OscEscape,
                (BellScan::Osc, _) => BellScan::Osc,
                // `ESC \` ends the sequence; another ESC starts a new one.
                (BellScan::OscEscape, b'\\') => BellScan::Text,
                (BellScan::Escape | BellScan::OscEscape, b']') => BellScan::Osc,
                (_, ESC) => BellScan::Escape,
                (_, BELL) => {
                    rang = true;
                    BellScan::Text
                }
                _ => BellScan::Text,
            };
        }
        rang
    }
}

/// The last line of the text with words in it, without escapes and cut for a reason.
pub fn last_line(text: &str) -> Option<String> {
    without_escapes(text)
        .split(['\r', '\n'])
        .map(js_trim)
        .rfind(|line| !line.is_empty())
        .map(quoted)
}

/// A line cut for a reason.
pub fn quoted(line: &str) -> String {
    line.chars().take(QUOTE_CHARS).collect()
}

/// The last `units` UTF-16 units of the text.
pub fn tail(text: String, units: usize) -> String {
    if utf16_len(&text) <= units {
        return text;
    }
    let mut used = 0;
    let mut start = text.len();
    for (index, c) in text.char_indices().rev() {
        used += c.len_utf16();
        if used > units {
            break;
        }
        start = index;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_failed_mark_but_not_its_typed_text() {
        let typed = "$Host.UI.RawUI.WindowTitle = 'natamux-agent-failed'";
        assert_eq!(failed_mark_at(typed), None);
        assert_eq!(failed_mark_at("ab\x1b]2;natamux-agent-failed\x07"), Some(2));
        assert_eq!(failed_mark_at("\x1b]0;natamux-agent-failed\x07"), Some(0));
        assert_eq!(failed_mark_at("\x1b]2;natamux-agent-exited\x07"), None);
    }

    #[test]
    fn finds_the_first_failed_mark_among_other_titles() {
        let other = "\x1b]2;zsh\x07é\x1b]1;natamux-agent-failed\x07";
        let mark = "\x1b]0;natamux-agent-failed\x07";
        let text = format!("{other}{mark}\x1b]2;natamux-agent-failed");
        assert_eq!(failed_mark_at(&text), Some(other.len()));
        assert_eq!(failed_mark_at("\x1b]"), None);
        assert_eq!(failed_mark_at("\x1b]2;natamux-agent-fail"), None);
    }

    /// `not_found_in` as it was: a lower-case copy of each line.
    fn not_found_by_copy(plain: &str, program: &str) -> Option<String> {
        let name = program.rsplit(['\\', '/']).next().unwrap_or(program);
        let mut lines = plain.split(['\r', '\n']).map(js_trim);
        let found = lines.find(|line| {
            let lower = line.to_lowercase();
            NOT_FOUND.iter().any(|words| lower.contains(words)) && line.contains(name)
        });
        found.map(String::from)
    }

    #[test]
    fn finds_the_not_found_line_in_any_case_as_a_lower_case_copy_does() {
        for (text, found) in [
            ("zsh: command not found: claude\r\n$ ", true),
            ("  bash: claude: Command Not Found  \r\n", true),
            ("'claude' IS NOT RECOGNIZED as a command\r\n", true),
            ("sh: 1: claude: not found", true),
            ("fish: Unknown command: claude\r\n", true),
            // Not ASCII: the Kelvin sign is a `k` in lower case, and the line is found as before.
            ("fish: Un\u{212a}nown command: claude → ?\r\n", true),
            ("é: command not found: claude", true),
            ("zsh: command not found: codex\r\n", false),
            ("claude\r\ncommand not found\r\n", false),
            ("claude: not foun", false),
            ("", false),
        ] {
            let line = not_found_in(text, "/usr/local/bin/claude");
            assert_eq!(line, not_found_by_copy(text, "/usr/local/bin/claude"));
            assert_eq!(line.is_some(), found, "{text}");
        }
    }

    // Bytes of Claude Code 2.1.319's first screen and Codex 0.167.0's sign-in screen.
    #[test]
    fn keeps_the_words_that_cursor_moves_put_apart() {
        let claude = "\x1b[2G\x1b[1mChoose\x1b[9Gthe\x1b[13Gtext\x1b[18Gstyle\x1b[22m";
        assert_eq!(without_escapes(claude), " Choose the text style");
        let codex =
            "\x1b[3;26Huse\x1b[3;30HCodex.\x1b[5;1H\x1b[2m> 1. \x1b[22mSign in with ChatGPT";
        assert_eq!(
            without_escapes(codex),
            " use Codex. > 1. Sign in with ChatGPT"
        );
    }

    #[test]
    fn gives_the_last_line_with_words_without_colors() {
        let text = "\x1b[31mError: not authenticated\x1b[0m\r\n  \r\n";
        assert_eq!(last_line(text).as_deref(), Some("Error: not authenticated"));
        assert_eq!(last_line("\r\n \r\n"), None);
        assert_eq!(
            last_line(&"x".repeat(500)).map(|line| line.len()),
            Some(200)
        );
    }

    #[test]
    fn rings_for_a_bell_but_not_for_the_end_of_a_title() {
        let rings = |chunks: &[&str]| {
            let mut scan = BellScan::default();
            chunks.iter().any(|chunk| scan.rings(chunk.as_bytes()))
        };
        assert!(rings(&["Allow? \x07"]));
        assert!(!rings(&["\x1b]2;natamux-agent-failed\x07"]));
        assert!(!rings(&["\x1b]2;natamux-agent-", "failed\x07"]));
        assert!(!rings(&["\x1b", "]0;title\x07"]));
        assert!(!rings(&["\x1b]0;title\x1b\\"]));
        assert!(rings(&["\x1b]0;title\x07", "\x07"]));
        assert!(rings(&["\x1b[31mred\x07"]));
    }
}
