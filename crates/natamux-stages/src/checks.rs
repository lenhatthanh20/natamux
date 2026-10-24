//! Ports `packages/daemon/src/stages/stage-checks.ts`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use natamux_core::js::{is_js_space, js_trim, parse_iso_millis};
use natamux_proto::stages::schema::Stage;
use natamux_store::task_files::STAGE_DIR;

use crate::git::{GitError, commit_count, dirty_files};
use natamux_git::process::capture;

pub use crate::git::current_head;

/// A `check` command gets this long before it counts as failed.
pub const CHECK_TIMEOUT_MS: u64 = 10 * 60 * 1000;
pub const OUTPUT_TAIL_CHARS: usize = 2000;
/// The end of the output of a command that ran too long.
pub const TIMED_OUT: &str = "(timed out)";
/// File times come from a coarser clock than the wall clock (on Linux, up to a few ms behind), so
/// a file written right after the stage started can look older than the start.
const FILE_TIME_SLACK_MS: f64 = 1000.0;
const MAX_COMMAND_OUTPUT: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckResult {
    pub passed: bool,
    /// Why it failed, one line each, for the agent and for you.
    pub reasons: Vec<String>,
    /// What passed, one line each, for the evidence on the progress page.
    pub passes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CheckInput {
    pub stage: Stage,
    /// The folder of the stage files: in the worktree, or the task's archive without one.
    pub folder: PathBuf,
    /// Where git and the `check` command run: the root of the worktree.
    pub cwd: PathBuf,
    /// Whether `cwd` is a git worktree; commit and clean checks need one.
    pub is_worktree: bool,
    /// When the stage started (ISO), and the commit HEAD was on then.
    pub started_at: Option<String>,
    pub start_head: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ran {
    pub code: i32,
    pub output: String,
}

pub type RunCommand = Arc<dyn Fn(&str, &Path) -> Ran + Send + Sync>;

pub fn default_run_command() -> RunCommand {
    Arc::new(run_shell_command)
}

/// A `check` command runs in the shell of the platform. cmd.exe reads the quoted command as it
/// is, the way Node runs a command with `shell: true`.
pub fn shell_command(command: &str, windows: bool) -> (String, Vec<String>) {
    if windows {
        (
            "cmd.exe".into(),
            ["/d", "/s", "/c"]
                .iter()
                .map(|arg| arg.to_string())
                .chain([format!("\"{command}\"")])
                .collect(),
        )
    } else {
        ("sh".into(), vec!["-c".into(), command.into()])
    }
}

pub fn run_shell_command(command: &str, cwd: &Path) -> Ran {
    run_shell_command_with_timeout(command, cwd, Duration::from_millis(CHECK_TIMEOUT_MS))
}

pub fn run_shell_command_with_timeout(command: &str, cwd: &Path, timeout: Duration) -> Ran {
    let mut process = Command::new(if cfg!(windows) { "cmd.exe" } else { "sh" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process
            .args(["/d", "/s", "/c"])
            .raw_arg(format!("\"{command}\""));
    }
    #[cfg(not(windows))]
    process.args(["-c", command]);
    process.current_dir(cwd);
    let ran = capture(process, timeout, MAX_COMMAND_OUTPUT);
    let output = format!("{}{}", ran.stdout, ran.stderr);
    if ran.code == Some(0) && !ran.overflow {
        return Ran { code: 0, output };
    }
    // Node adds the line for any kill, so the output limit gets it too.
    let output = if ran.killed() {
        format!("{output}\n{TIMED_OUT}")
    } else {
        output
    };
    // A kill, a signal or a program that did not start has no code: 1, and never 0.
    let code = match ran.code {
        Some(code) if !ran.killed() && code != 0 => code,
        _ => 1,
    };
    Ran { code, output }
}

/// The text of a heading line, as the regex `^#{1,6}\s+(.+?)\s*#*\s*$` captures it.
fn heading_text(line: &str) -> Option<String> {
    let (hashes, rest) = hash_run(line)?;
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let body = rest.trim_start_matches(is_js_space);
    let spaces = rest.chars().count() - body.chars().count();
    if body.is_empty() {
        // `\s+` gives its last space back to the lazy group.
        let last = rest
            .chars()
            .last()
            .filter(|c| !matches!(c, '\r' | '\u{2028}' | '\u{2029}'))?;
        return (spaces >= 2).then(|| last.to_string());
    }
    let kept = body
        .trim_end_matches(is_js_space)
        .trim_end_matches('#')
        .trim_end_matches(is_js_space);
    let text = if kept.is_empty() {
        body.chars().next().map(String::from)?
    } else {
        kept.to_string()
    };
    Some(text)
}

/// The `#` run at the start of a line and what follows it, when a space follows the run.
fn hash_run(line: &str) -> Option<(usize, &str)> {
    let rest = line.trim_start_matches('#');
    let hashes = line.len() - rest.len();
    rest.chars().next().filter(|c| is_js_space(*c))?;
    Some((hashes, rest))
}

fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// The headings of a markdown text, without their # marks, in lower case.
pub fn headings(text: &str) -> Vec<String> {
    lines_of(text)
        .into_iter()
        .filter_map(|line| heading_text(line).map(|heading| heading.to_lowercase()))
        .collect()
}

/// The lines under a heading, up to the next heading of the same or a higher level.
pub fn section_body(text: &str, name: &str) -> Vec<String> {
    let lines = lines_of(text);
    let name = name.to_lowercase();
    let Some(start) = lines
        .iter()
        .position(|line| heading_text(line).is_some_and(|found| found.to_lowercase() == name))
    else {
        return Vec::new();
    };
    let level = lines[start]
        .chars()
        .take_while(|c| *c == '#')
        .count()
        .max(1);
    let mut body = Vec::new();
    for line in &lines[start + 1..] {
        if hash_run(line).is_some_and(|(deeper, _)| deeper > 0 && deeper <= level) {
            break;
        }
        body.push((*line).to_string());
    }
    body
}

fn is_list_item(line: &str) -> bool {
    let after_marker = if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        rest
    } else {
        let rest = line.trim_start_matches(|c: char| c.is_ascii_digit());
        if rest.len() == line.len() {
            return false;
        }
        match rest.strip_prefix(['.', ')']) {
            Some(after) => after,
            None => return false,
        }
    };
    after_marker.starts_with(is_js_space)
        && !after_marker.trim_start_matches(is_js_space).is_empty()
}

/// A case is a sub-heading under the section, or, without sub-headings, a list item that is not
/// indented (the input and expected result of a case are often a list of their own).
pub fn count_items(text: &str, section: &str) -> usize {
    let body = section_body(text, section);
    let sub = body
        .iter()
        .filter(|line| hash_run(line).is_some_and(|(hashes, _)| (2..=6).contains(&hashes)))
        .count();
    if sub > 0 {
        return sub;
    }
    body.iter().filter(|line| is_list_item(line)).count()
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn modified_ms(path: &Path) -> Option<f64> {
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs_f64() * 1000.0,
        Err(before) => -(before.duration().as_secs_f64() * 1000.0),
    })
}

fn check_output(input: &CheckInput, result: &mut CheckResult) {
    let stage = &input.stage;
    let Some(output) = &stage.output else {
        return;
    };
    let path = input.folder.join(output);
    let read = std::fs::read(&path).ok().zip(modified_ms(&path));
    let Some((bytes, modified)) = read else {
        result
            .reasons
            .push(format!("{STAGE_DIR}/{output} is missing"));
        return;
    };
    let text = String::from_utf8_lossy(&bytes);
    if js_trim(&text).is_empty() {
        result.reasons.push(format!("{output} is empty"));
        return;
    }
    let started = input.started_at.as_deref().and_then(parse_iso_millis);
    if started.is_some_and(|started| modified < started as f64 - FILE_TIME_SLACK_MS) {
        result
            .reasons
            .push(format!("{output} was not changed after the stage started"));
        return;
    }
    result
        .passes
        .push(format!("{output} was written after the stage started"));

    let found = headings(&text);
    let sections = stage.sections.as_deref().unwrap_or(&[]);
    let missing: Vec<&str> = sections
        .iter()
        .filter(|name| !found.contains(&name.to_lowercase()))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        result
            .reasons
            .push(format!("{output} has no {} section", missing.join(", ")));
    } else if !sections.is_empty() {
        result
            .passes
            .push(format!("Sections {} are there", sections.join(", ")));
    }
    if sections.iter().any(|name| name.to_lowercase() == "cases") {
        let cases = count_items(&text, "Cases");
        if cases == 0 {
            result
                .reasons
                .push(format!("{output} has no case under Cases"));
        } else {
            result.passes.push(format!(
                "Section Cases has {}",
                plural(cases, "case", "cases")
            ));
        }
    }
}

fn check_git(input: &CheckInput, result: &mut CheckResult) -> Result<(), GitError> {
    let stage = &input.stage;
    let require_commit = stage.require_commit == Some(true);
    let require_clean = stage.require_clean == Some(true);
    if !input.is_worktree {
        if require_commit || require_clean {
            result.reasons.push("The task has no workspace".into());
        }
        return Ok(());
    }
    if require_commit {
        let range = input
            .start_head
            .as_ref()
            .map_or_else(|| "HEAD".to_string(), |head| format!("{head}..HEAD"));
        let count = commit_count(&input.cwd, &range)?;
        if count == 0 {
            result
                .reasons
                .push("There is no new commit since the stage started".into());
        } else {
            result.passes.push(format!(
                "{} since the stage started",
                plural(count as usize, "new commit", "new commits")
            ));
        }
    }
    if require_clean {
        let dirty = dirty_files(&input.cwd)?;
        if dirty.is_empty() {
            result.passes.push("Every change is committed".into());
        } else {
            let count = dirty.len();
            result.reasons.push(format!(
                "{count} {} not committed",
                if count == 1 { "file is" } else { "files are" }
            ));
        }
    }
    Ok(())
}

/// The last `count` UTF-16 units of the text, as `slice(-count)`.
pub(crate) fn tail_chars(text: &str, count: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16_lossy(&units[units.len().saturating_sub(count)..])
}

/// What the daemon checks after a stage reports done (docs/design-task-stages.md 3.5). Errors:
/// a git call that failed (Node's check throws).
pub fn run_stage_checks(
    input: &CheckInput,
    run_command: &RunCommand,
) -> Result<CheckResult, GitError> {
    let mut result = CheckResult::default();
    check_output(input, &mut result);
    check_git(input, &mut result)?;
    if let Some(check) = &input.stage.check
        && result.reasons.is_empty()
    {
        let ran = run_command(check, &input.cwd);
        if ran.code == 0 {
            result.passes.push(format!("{check} passed"));
        } else {
            result.reasons.push(format!(
                "{check} failed with exit code {}:\n{}",
                ran.code,
                tail_chars(&ran.output, OUTPUT_TAIL_CHARS)
            ));
        }
    }
    result.passed = result.reasons.is_empty();
    Ok(result)
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
