//! The hook every crate calls when it blocks something or changes security state. The daemon
//! installs the sink that writes `logs/audit.log`; with no sink a call does nothing. Only short
//! fixed words pass: an id or a result that holds anything else is replaced, so a title, a reason
//! or a URL never reaches the log.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

/// `(event, id, result)`, already cleaned.
pub type AuditSink = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

/// What an id or a result that is not a plain word becomes.
pub const NOT_A_WORD: &str = "-";
const MAX_WORD: usize = 80;

static INSTALLED: Mutex<Option<AuditSink>> = Mutex::new(None);

thread_local! {
    // Per thread, so a test that catches the lines never sees another test's.
    static FORCED: RefCell<Option<AuditSink>> = const { RefCell::new(None) };
}

/// `text` when it is a plain word: a task id, a host, a stage or a fixed result. Else `-`.
pub fn clean_word(text: &str) -> String {
    let plain = !text.is_empty()
        && text.len() <= MAX_WORD
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/@".contains(&byte));
    if plain {
        text.to_string()
    } else {
        NOT_A_WORD.to_string()
    }
}

/// Makes `sink` get every `record` of this process.
pub fn install(sink: AuditSink) {
    if let Ok(mut installed) = INSTALLED.lock() {
        *installed = Some(sink);
    }
}

/// One audit line: `event` from a fixed list, `id` the task or host, `result` a fixed word.
pub fn record(event: &str, id: &str, result: &str) {
    let sink = FORCED
        .with(|forced| forced.borrow().clone())
        .or_else(|| INSTALLED.lock().ok().and_then(|it| it.clone()));
    if let Some(sink) = sink {
        sink(&clean_word(event), &clean_word(id), &clean_word(result));
    }
}

/// Test hook: `record` on this thread goes to `sink` until the guard is dropped.
pub fn force(sink: AuditSink) -> ForcedAudit {
    let previous = FORCED.with(|cell| cell.replace(Some(sink)));
    ForcedAudit { previous }
}

#[must_use = "the sink goes back when the guard is dropped"]
pub struct ForcedAudit {
    previous: Option<AuditSink>,
}

impl Drop for ForcedAudit {
    fn drop(&mut self) {
        let previous = self.previous.take();
        FORCED.with(|cell| cell.replace(previous));
    }
}

/// Test helper: a sink that keeps the lines, and the list it keeps them in.
pub fn catcher() -> (AuditSink, Arc<Mutex<Vec<[String; 3]>>>) {
    let lines: Arc<Mutex<Vec<[String; 3]>>> = Arc::default();
    let kept = Arc::clone(&lines);
    let sink: AuditSink = Arc::new(move |event, id, result| {
        if let Ok(mut lines) = kept.lock() {
            lines.push([event.to_string(), id.to_string(), result.to_string()]);
        }
    });
    (sink, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words_pass_and_free_text_is_replaced() {
        assert_eq!(clean_word("T-0042"), "T-0042");
        assert_eq!(clean_word("github.com"), "github.com");
        assert_eq!(clean_word("Secret MARK title"), NOT_A_WORD);
        assert_eq!(clean_word("https://u:p@h/x?token=1"), NOT_A_WORD);
        assert_eq!(clean_word(""), NOT_A_WORD);
        assert_eq!(clean_word(&"a".repeat(81)), NOT_A_WORD);
    }

    #[test]
    fn record_goes_to_the_forced_sink_cleaned_and_nowhere_after_the_guard() {
        let (sink, lines) = catcher();
        {
            let _forced = force(sink);
            record("agent.blocked", "T-1", "blocked");
            record("agent.blocked", "reason with MARK", "blocked");
        }
        record("agent.blocked", "T-2", "blocked");
        let lines = lines.lock().unwrap().clone();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], ["agent.blocked", "T-1", "blocked"]);
        assert_eq!(lines[1], ["agent.blocked", "-", "blocked"]);
    }
}
