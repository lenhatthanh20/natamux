//! Ports the pure half of `packages/daemon/src/git/commit-drafter.ts`: the prompt, the cleaning of
//! what the agent printed, and the draft with its fallback. Running the agent (`runAgentOnce`,
//! `shellProcess`) is in the daemon, which knows the shell.

use std::path::PathBuf;

use natamux_core::git::commit_message::{cut_commit_subject, simple_commit_draft};
use natamux_core::js::{is_js_space, js_trim};
use natamux_proto::workspaces::changes::{CommitMessageDraft, DraftSource};
use natamux_proto::workspaces::workspace::ChangeEntry;

use crate::commit_push::utf16_prefix;

pub const DRAFT_TIMEOUT_MS: u64 = 60_000;
/// The diff in the prompt. Agents answer slower, and cost more, the longer it is.
pub const MAX_DIFF_CHARS: usize = 60_000;
/// The limit of a commit message in `POST /tasks/:id/commit`.
pub const MAX_MESSAGE_CHARS: usize = 10_000;
pub const MAX_OUTPUT_CHARS: usize = 1024 * 1024;

/// Without these, the agent's hooks could report to the daemon and move the task.
pub const DAEMON_VARIABLES: [&str; 5] = [
    "NATAMUX_TOKEN",
    "NATAMUX_PORT",
    "NATAMUX_URL",
    "NATAMUX_TASK_ID",
    "NATAMUX_TERMINAL",
];

/// One run of an agent for a message: what `run` of `draft_commit_message` gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRun {
    pub agent: String,
    pub program: String,
    pub prompt: String,
    pub cwd: PathBuf,
    pub timeout_ms: u64,
}

pub struct DraftInput {
    pub agent: String,
    pub program: String,
    pub cwd: PathBuf,
    pub title: String,
    pub changes: Vec<ChangeEntry>,
    pub diff: String,
    /// None: `DRAFT_TIMEOUT_MS`.
    pub timeout_ms: Option<u64>,
}

pub fn commit_prompt(title: &str, diff: &str) -> String {
    [
        "Write one git commit message for the diff below. Output the message only.",
        "- First line: imperative mood, at most 72 characters, no period at the end.",
        "- Then, only when the change needs it: a blank line and short lines that say why.",
        "- Simple English. No Markdown, no code fences, no quotes around the message.",
        "- No Co-Authored-By or \"Generated with\" lines, no mention of an AI or an agent.",
        "- Do not run commands or change files.",
        "",
        &format!("Task: {title}"),
        "",
        diff,
    ]
    .join("\n")
}

/// Where the escape sequence that starts at `start` (an ESC) ends, as Node's regex
/// `\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)` matches it.
fn escape_end(chars: &[char], start: usize) -> Option<usize> {
    let mut at = start + 2;
    match chars.get(start + 1)? {
        '[' => {
            while chars
                .get(at)
                .is_some_and(|c| c.is_ascii_digit() || *c == ';' || *c == '?')
            {
                at += 1;
            }
            chars
                .get(at)
                .filter(|c| c.is_ascii_alphabetic())
                .map(|_| at + 1)
        }
        ']' => {
            while chars
                .get(at)
                .is_some_and(|c| *c != '\u{7}' && *c != '\u{1b}')
            {
                at += 1;
            }
            match chars.get(at)? {
                '\u{7}' => Some(at + 1),
                '\u{1b}' if chars.get(at + 1) == Some(&'\\') => Some(at + 2),
                _ => None,
            }
        }
        _ => None,
    }
}

fn strip_escapes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\u{1b}'
            && let Some(end) = escape_end(&chars, at)
        {
            at = end;
            continue;
        }
        out.push(chars[at]);
        at += 1;
    }
    out
}

/// The inside of a code fence that wraps the whole text, as Node's regex
/// `^```[^\n]*\n([\s\S]*?)\n```$` reads it.
fn unfence(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("```")?;
    let (_, after_first_line) = rest.split_once('\n')?;
    after_first_line.strip_suffix("\n```")
}

/// The inside of a pair of quotes that wraps the whole text, as Node's regex
/// `^(["'`])([\s\S]*)\1$` reads it.
fn unquote(text: &str) -> Option<&str> {
    let first = text
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\'' | '`'))?;
    text.strip_prefix(first)?.strip_suffix(first)
}

/// Node's regex `^co-authored-by:|^\W*generated with`, case-insensitive. `\W` is ASCII only.
fn is_attribution(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("co-authored-by:") {
        return true;
    }
    lower
        .trim_start_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .starts_with("generated with")
}

/// Turns what the agent printed into a commit message. Empty when nothing is left.
pub fn clean_commit_message(output: &str) -> String {
    let stripped = strip_escapes(output).replace("\r\n", "\n");
    let mut text = js_trim(&stripped);
    if let Some(inside) = unfence(text) {
        text = js_trim(inside);
    }
    if let Some(inside) = unquote(text) {
        text = js_trim(inside);
    }
    let mut lines = text
        .split('\n')
        .filter(|line| !is_attribution(js_trim(line)));
    let subject = lines.next().unwrap_or("");
    let mut parts = vec![cut_commit_subject(js_trim(subject))];
    parts.extend(lines.map(str::to_string));
    let joined = parts.join("\n");
    let (kept, _) = utf16_prefix(js_trim(&joined), MAX_MESSAGE_CHARS);
    js_trim(kept).to_string()
}

/// The first word of a program line, for the texts that name the agent.
pub fn name_of(program: &str) -> &str {
    js_trim(program)
        .split(is_js_space)
        .next()
        .unwrap_or(program)
}

/// Asks the agent for a commit message. When it cannot give one, the simple draft takes its place
/// and `reason` says why.
pub fn draft_commit_message(
    input: &DraftInput,
    run: &dyn Fn(AgentRun) -> Result<String, String>,
) -> CommitMessageDraft {
    let answer = run(AgentRun {
        agent: input.agent.clone(),
        program: input.program.clone(),
        prompt: commit_prompt(&input.title, &input.diff),
        cwd: input.cwd.clone(),
        timeout_ms: input.timeout_ms.unwrap_or(DRAFT_TIMEOUT_MS),
    });
    let reason = match answer {
        Ok(output) => {
            let message = clean_commit_message(&output);
            if !message.is_empty() {
                return CommitMessageDraft {
                    message,
                    source: DraftSource::Agent,
                    agent: input.agent.clone(),
                    reason: None,
                };
            }
            format!("{} printed no message", name_of(&input.program))
        }
        Err(reason) => reason,
    };
    CommitMessageDraft {
        message: simple_commit_draft(&input.title, &input.changes),
        source: DraftSource::Draft,
        agent: input.agent.clone(),
        reason: Some(reason),
    }
}

#[cfg(test)]
#[path = "commit_drafter_tests.rs"]
mod tests;
