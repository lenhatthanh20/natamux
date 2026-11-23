//! Words that would turn off an agent's approvals. In the `ask` permission mode, and always in
//! Strict mode, a start with one of them fails with a reason: Natamux never drops a word and
//! starts the agent anyway. Flags as each CLI documents them (checked 2026-11-23).

use crate::launch::auto_flags;

/// A word that turns off approvals, and why the start fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forbidden {
    pub word: String,
    pub reason: String,
}

/// When a flag is unsafe.
#[derive(Clone, Copy, Debug)]
enum Unsafe {
    /// With any value or none.
    Always,
    /// With this value.
    When(&'static str),
    /// With one of these values.
    WhenAny(&'static [&'static str]),
    /// Unless it has one of these values (no value counts as unsafe).
    Unless(&'static [&'static str]),
}

/// The flags beyond `AUTO_FLAGS` that let each agent act without asking.
fn extras(agent: &str) -> &'static [(&'static str, Unsafe)] {
    use Unsafe::{Always, Unless, When, WhenAny};
    match agent {
        "claude-code" => &[
            ("--dangerously-skip-permissions", Always),
            ("--allow-dangerously-skip-permissions", Always),
            ("--permission-mode", Unless(&["default", "plan"])),
        ],
        "codex" => &[
            ("--dangerously-bypass-approvals-and-sandbox", Always),
            ("--yolo", Always),
            ("--full-auto", Always),
            ("--ask-for-approval", When("never")),
            ("-a", When("never")),
            ("--sandbox", When("danger-full-access")),
            ("-s", When("danger-full-access")),
        ],
        "gemini" | "qwen" => &[
            ("--yolo", Always),
            ("-y", Always),
            ("--approval-mode", WhenAny(&["yolo", "auto_edit"])),
        ],
        "copilot" => &[("--yolo", Always)],
        "cursor" => &[("--force", Always), ("-f", Always), ("--yolo", Always)],
        "droid" => &[("--skip-permissions-unsafe", Always)],
        "kimi" => &[("--afk", Always)],
        "cline" => &[("--yolo", Always)],
        "kiro" => &[("--trust-tools", Always)],
        _ => &[],
    }
}

/// `AUTO_FLAGS` of the agent as rules: a flag followed by a plain word is unsafe with that value.
fn auto_rules(agent: &str) -> Vec<(&'static str, Unsafe)> {
    let flags = auto_flags(agent);
    let mut rules = Vec::new();
    let mut index = 0;
    while let Some(flag) = flags.get(index) {
        match flags.get(index + 1).filter(|next| !next.starts_with('-')) {
            Some(value) => {
                rules.push((*flag, Unsafe::When(value)));
                index += 2;
            }
            None => {
                rules.push((*flag, Unsafe::Always));
                index += 1;
            }
        }
    }
    rules
}

/// The words as a POSIX shell splits them: quotes and backslashes removed, so `'--yolo'` and
/// `--yo""lo` are both `--yolo`. A PowerShell `'a''b'` gives `ab`, which is fine for flags.
pub fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(open) if c == open => quote = None,
            Some('"') if c == '\\' => word.extend(chars.next()),
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                in_word = true;
            }
            None if c == '\\' => {
                word.extend(chars.next());
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

fn unsafe_with(rule: Unsafe, value: Option<&str>) -> bool {
    match rule {
        Unsafe::Always => true,
        Unsafe::When(bad) => value == Some(bad),
        Unsafe::WhenAny(bad) => value.is_some_and(|value| bad.contains(&value)),
        Unsafe::Unless(good) => value.is_none_or(|value| !good.contains(&value)),
    }
}

/// The first word in `words` that turns off approvals for `agent` (a built-in kind), as
/// written: `--flag=value`, or `--flag value` when the value is the next word.
pub fn forbidden_word(agent: &str, words: &str) -> Option<Forbidden> {
    let mut rules = auto_rules(agent);
    rules.extend_from_slice(extras(agent));
    let words = split_words(words);
    for (index, word) in words.iter().enumerate() {
        let hit = if word.contains("GOOSE_MODE") {
            Some(word.clone())
        } else {
            hit_of(agent, &rules, word, words.get(index + 1))
        };
        if let Some(word) = hit {
            let reason = format!(
                "\"{word}\" would let {agent} act without asking, so it does not start in the ask \
                 permission mode. Remove the word, or set the permission mode to auto."
            );
            return Some(Forbidden { word, reason });
        }
    }
    None
}

fn hit_of(
    agent: &str,
    rules: &[(&'static str, Unsafe)],
    word: &str,
    next: Option<&String>,
) -> Option<String> {
    let (flag, inline) = match word.split_once('=') {
        Some((flag, value)) if word.starts_with("--") => (flag, Some(value)),
        _ => (word, None),
    };
    if agent == "copilot" && flag.starts_with("--allow-all") {
        return Some(word.to_string());
    }
    let next = next
        .map(String::as_str)
        .filter(|next| !next.starts_with('-'));
    let value = inline.or(next);
    let rule = rules
        .iter()
        .filter(|(name, _)| *name == flag)
        .map(|(_, rule)| *rule)
        .find(|rule| unsafe_with(*rule, value))?;
    // A value taken from the next word is part of what the user wrote.
    match (inline, next, rule) {
        (None, Some(next), Unsafe::When(_) | Unsafe::WhenAny(_) | Unsafe::Unless(_)) => {
            Some(format!("{word} {next}"))
        }
        _ => Some(word.to_string()),
    }
}

#[cfg(test)]
#[path = "launch_policy_tests.rs"]
mod tests;
