//! A trace of terminal input, on only with `NATAMUX_INPUT_TRACE=1`: what a client sent and what
//! went to the shell, with the time between them. It shows which bytes an input method (such as a
//! Vietnamese one) sends, when typed text comes out wrong in an agent. It logs typed text,
//! passwords too, so it is off by default.

use natamux_pty::input_gap::InputKind;

use super::session_lifecycle::session_key;
use super::types::Session;

/// Most characters of one line, so a paste does not flood the log.
const MAX_CHARS: usize = 200;

/// Only exactly `1` turns the trace on: it logs passwords, so nothing else may.
pub fn trace_input_on(value: Option<&str>) -> bool {
    value == Some("1")
}

#[derive(Clone, Copy)]
pub enum Step {
    /// An `input` message from a client, before the DEL split.
    Recv,
    /// One write to the pty.
    Write(InputKind),
}

/// `input T-1/1 recv +25 "a\u{7f}"` or `input T-1/1 write +25 erase "\u{7f}"`: `since` is the
/// time in ms since the line before, and the text has its control characters escaped.
pub fn line(key: &str, step: Step, since: u64, data: &str) -> String {
    let mut text = format!("{:?}", data.chars().take(MAX_CHARS).collect::<String>());
    if data.chars().nth(MAX_CHARS).is_some() {
        text.push('…');
    }
    match step {
        Step::Recv => format!("input {key} recv +{since} {text}"),
        Step::Write(kind) => {
            let kind = match kind {
                InputKind::Text => "text",
                InputKind::Erase => "erase",
                InputKind::Submit => "submit",
            };
            format!("input {key} write +{since} {kind} {text}")
        }
    }
}

/// The line to log, or none when the trace is off. `traced_at` is when the last line was
/// logged; the first line has `+0`.
pub fn next_line(
    on: bool,
    traced_at: &mut Option<u64>,
    now: u64,
    key: &str,
    step: Step,
    data: &str,
) -> Option<String> {
    if !on {
        return None;
    }
    let since = traced_at.map_or(0, |at| now.saturating_sub(at));
    *traced_at = Some(now);
    Some(line(key, step, since, data))
}

/// Logs to stderr, which the app writes to the daemon log.
pub fn trace(session: &mut Session, step: Step, data: &str) {
    if !session.trace_input {
        return;
    }
    let now = session.now();
    let key = session_key(&session.task_id, &session.terminal);
    if let Some(line) = next_line(true, &mut session.traced_at, now, &key, step, data) {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_characters_and_names_the_step() {
        assert_eq!(
            line("T-1/1", Step::Recv, 0, "ro"),
            r#"input T-1/1 recv +0 "ro""#
        );
        assert_eq!(
            line("T-1/1", Step::Write(InputKind::Erase), 25, "\x7f"),
            r#"input T-1/1 write +25 erase "\u{7f}""#
        );
        assert_eq!(
            line("T-1/1", Step::Write(InputKind::Text), 25, "ô\x1b[A"),
            r#"input T-1/1 write +25 text "ô\u{1b}[A""#
        );
        assert_eq!(
            line("T-1/1", Step::Write(InputKind::Submit), 100, "\r"),
            r#"input T-1/1 write +100 submit "\r""#
        );
    }

    #[test]
    fn turns_on_only_for_exactly_1() {
        assert!(trace_input_on(Some("1")));
        for value in [
            None,
            Some("0"),
            Some("true"),
            Some("yes"),
            Some(""),
            Some(" 1"),
            Some("1 "),
        ] {
            assert!(!trace_input_on(value), "{value:?}");
        }
    }

    #[test]
    fn counts_the_time_since_the_line_before_only_while_on() {
        let mut at = None;
        assert_eq!(
            next_line(false, &mut at, 100, "T-1/1", Step::Recv, "a"),
            None
        );
        assert_eq!(at, None);
        assert_eq!(
            next_line(true, &mut at, 100, "T-1/1", Step::Recv, "a"),
            Some(r#"input T-1/1 recv +0 "a""#.to_string())
        );
        assert_eq!(
            next_line(
                true,
                &mut at,
                125,
                "T-1/1",
                Step::Write(InputKind::Text),
                "a"
            ),
            Some(r#"input T-1/1 write +25 text "a""#.to_string())
        );
        assert_eq!(at, Some(125));
    }

    #[test]
    fn cuts_long_text_at_200_characters() {
        let exact = "ồ".repeat(MAX_CHARS);
        assert_eq!(
            line("T-1/1", Step::Recv, 0, &exact),
            format!("input T-1/1 recv +0 \"{exact}\"")
        );
        assert_eq!(
            line("T-1/1", Step::Recv, 0, &format!("{exact}x")),
            format!("input T-1/1 recv +0 \"{exact}\"…")
        );
    }
}
