//! Git that the daemon runs while isolation is on: a worktree's agent can write files the daemon's
//! git would read (a plain checkout's `.git/config`, or a git folder it points `.git` at), so git
//! runs no hook, no fsmonitor command and no `ext::` remote, and skips the system config. Before
//! each call, `harden_git_in` also replaces every setting that names a command when it comes from
//! the repo's own config. Process-wide: isolation is set once, at the daemon's start.
//!
//! An isolated worktree's `.git` file is the agent's too: its git folders, checked by the
//! isolation plan, are pinned here and passed to git as `GIT_DIR`, `GIT_COMMON_DIR` and
//! `GIT_WORK_TREE`, so git never follows that file.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, PoisonError};

static EMPTY_HOOKS: OnceLock<PathBuf> = OnceLock::new();

/// An isolated worktree (canonical) and its checked git folder and shared folder. None: the
/// check failed.
pub type Pinned = (PathBuf, Option<(PathBuf, PathBuf)>);
static PINNED: Mutex<Vec<Pinned>> = Mutex::new(Vec::new());

/// The protocols git may use while the hardening is on: no remote helper (`foo::url`,
/// `remote.<n>.vcs`), which is a program the repo names.
const ALLOWED_PROTOCOLS: &str = "https:ssh:file:git";
/// What a repo `core.gitproxy` gets (see `overrides_of`).
const NO_GIT_PROTOCOL: &str = "protocol.git.allow=never";

/// The settings that name a command git may run (POSIX extended regex over git's key names).
const COMMAND_KEYS: &str = concat!(
    r"^(filter\..+\.(clean|smudge|process)|diff\..+\.(command|textconv)|diff\.external",
    r"|merge\..+\.driver|core\.(sshcommand|askpass|gitproxy|pager|editor|alternaterefscommand)",
    r"|sequence\.editor|gpg\.(.+\.)?program|gpg\.ssh\.defaultkeycommand",
    r"|remote\..+\.(uploadpack|receivepack)|credential\.(.+\.)?helper)$"
);

/// Turns the hardening on for the rest of the process. `empty_hooks`: a folder with no hook.
pub fn install_git_hardening(empty_hooks: PathBuf) {
    let _ = EMPTY_HOOKS.set(empty_hooks);
}

/// The settings of the hardening, while it is on.
pub fn hardening_config() -> Vec<(String, String)> {
    let Some(hooks) = EMPTY_HOOKS.get() else {
        return Vec::new();
    };
    vec![
        ("core.hooksPath".to_string(), hooks.display().to_string()),
        ("core.fsmonitor".to_string(), "false".to_string()),
        ("protocol.ext.allow".to_string(), "never".to_string()),
    ]
}

/// The arguments that go before git's own, while the hardening is on.
pub fn hardening_args() -> Vec<String> {
    let entries = hardening_config().into_iter();
    config_args(entries.map(|(key, value)| format!("{key}={value}")))
}

/// Keeps the git folders that the isolation plan checked for an isolated worktree; `dirs` None:
/// the check failed, so git there gets a git folder that does not exist. A pair checked before
/// is never replaced, by a failed check or by another pair: the agent may have built the new one
/// in a folder it writes. True when the pin now holds `dirs`.
pub fn pin_git_dirs(worktree: &Path, dirs: Option<(PathBuf, PathBuf)>) -> bool {
    let root = real(worktree);
    let mut pinned = PINNED.lock().unwrap_or_else(PoisonError::into_inner);
    match pinned.iter_mut().find(|(each, _)| *each == root) {
        Some((_, old)) if old.is_none() => *old = dirs,
        Some((_, old)) => return *old == dirs,
        None => pinned.push((root, dirs)),
    }
    true
}

/// Forgets the pin of a worktree that is removed, so one made again at that path is checked anew.
pub fn unpin_git_dirs(worktree: &Path) {
    // The folder may be gone already: its parent names it.
    let gone = worktree
        .parent()
        .zip(worktree.file_name())
        .map(|(parent, name)| real(parent).join(name));
    let roots = [Some(real(worktree)), gone];
    let mut pinned = PINNED.lock().unwrap_or_else(PoisonError::into_inner);
    pinned.retain(|(root, _)| !roots.iter().flatten().any(|each| each == root));
}

/// The isolated worktree that holds `cwd` and its pinned git folders. None when `cwd` is in no
/// isolated worktree.
pub fn pinned_git_dirs(cwd: &Path) -> Option<Pinned> {
    let pinned = PINNED.lock().unwrap_or_else(PoisonError::into_inner);
    if pinned.is_empty() {
        return None;
    }
    let cwd = real(cwd);
    pinned
        .iter()
        .filter(|(root, _)| cwd.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .cloned()
}

fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The variables that pin git in `cwd` to the checked git folders, while the hardening is on.
/// Empty when `cwd` is in no isolated worktree.
pub fn git_pins(cwd: &Path) -> Vec<(String, String)> {
    EMPTY_HOOKS
        .get()
        .map(|hooks| pins_of(hooks, cwd))
        .unwrap_or_default()
}

fn pins_of(hooks: &Path, cwd: &Path) -> Vec<(String, String)> {
    let text = |path: &Path| path.to_string_lossy().into_owned();
    match pinned_git_dirs(cwd) {
        None => Vec::new(),
        Some((root, Some((git_dir, common)))) => vec![
            ("GIT_DIR".to_string(), text(&git_dir)),
            ("GIT_COMMON_DIR".to_string(), text(&common)),
            // The top of the worktree, also for a call from a subfolder.
            ("GIT_WORK_TREE".to_string(), text(&root)),
        ],
        Some((_, None)) => vec![("GIT_DIR".to_string(), text(&hooks.join("refused")))],
    }
}

fn config_args(entries: impl IntoIterator<Item = String>) -> Vec<String> {
    entries
        .into_iter()
        .flat_map(|entry| ["-c".to_string(), entry])
        .collect()
}

/// Call right after `Command::new("git")`, before any other argument. Only for a git call that
/// runs no filter, diff, merge, signing or network (`rev-parse`, `cat-file --batch`): others use
/// `harden_git_in`.
pub fn harden_git(command: &mut Command) {
    let args = hardening_args();
    if !args.is_empty() {
        command.args(args).env("GIT_CONFIG_NOSYSTEM", "1");
        // A narrower list the caller set before stays.
        if env_of(command, "GIT_ALLOW_PROTOCOL").is_none() {
            command.env("GIT_ALLOW_PROTOCOL", ALLOWED_PROTOCOLS);
        }
    }
}

/// `harden_git`, and the pinned git folders when `cwd` is in an isolated worktree.
pub fn harden_git_at(command: &mut Command, cwd: &Path) {
    harden_git(command);
    command.envs(git_pins(cwd));
}

/// `harden_git_at`, and the repo's command settings replaced, for a git call in `cwd` with `env`.
/// It asks git which settings it would read; when that fails, the call gets a git folder that
/// does not exist, so it fails without reading the repo's config. Call it after the caller's
/// own variables: the pins and that refusal must win.
pub fn harden_git_in(command: &mut Command, cwd: &Path, env: &[(String, String)]) {
    let Some(hooks) = EMPTY_HOOKS.get() else {
        return;
    };
    harden_git(command);
    let pins = pins_of(hooks, cwd);
    command.envs(pins.iter().map(|(key, value)| (key, value)));
    // The probe reads the pinned git folders too, never the agent's `.git` file.
    let env: &[(String, String)] = &[env, pins.as_slice()].concat();
    match command_settings(cwd, env).and_then(|listing| overrides_of(&listing)) {
        Some(entries) => {
            // With GIT_ALLOW_PROTOCOL set, git skips `protocol.git.allow`.
            if entries.iter().any(|entry| entry == NO_GIT_PROTOCOL) && allows_git(command) {
                command.env("GIT_ALLOW_PROTOCOL", "https:ssh:file");
            }
            command.args(config_args(entries));
        }
        None => {
            command.env("GIT_DIR", hooks.join("refused"));
        }
    }
}

fn env_of(command: &Command, name: &str) -> Option<String> {
    let (_, value) = command.get_envs().find(|(key, _)| *key == name)?;
    Some(value?.to_string_lossy().into_owned())
}

fn allows_git(command: &Command) -> bool {
    let list = env_of(command, "GIT_ALLOW_PROTOCOL").unwrap_or_default();
    list.split(':').any(|name| name == "git")
}

/// `git config --show-scope -z --get-regexp` of the command keys, as git would read them.
fn command_settings(cwd: &Path, env: &[(String, String)]) -> Option<String> {
    let mut probe = Command::new("git");
    probe
        .args(["config", "--show-scope", "-z", "--get-regexp", COMMAND_KEYS])
        .current_dir(cwd)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    crate::login_path::apply(&mut probe);
    let output = probe.output().ok()?;
    // 1: no key matched.
    match output.status.code() {
        Some(0) => String::from_utf8(output.stdout).ok(),
        Some(1) => Some(String::new()),
        _ => None,
    }
}

/// Global and `-c`/`GIT_CONFIG_*` settings are the user's and the daemon's own; the repo's
/// (local, worktree, and what they include) are not.
fn trusted(scope: &str) -> bool {
    matches!(scope, "global" | "command")
}

/// `(scope, key, value)` of a `-z --show-scope` listing; None when it is not one.
fn entries_of(listing: &str) -> Option<Vec<(&str, &str, &str)>> {
    let fields: Vec<&str> = listing.split('\0').collect();
    let (last, fields) = fields.split_last()?;
    let (pairs, remainder) = fields.as_chunks::<2>();
    if !last.is_empty() || !remainder.is_empty() {
        return None;
    }
    let entries = pairs.iter().map(|&[scope, setting]| {
        let (key, value) = setting.split_once('\n').unwrap_or((setting, ""));
        (scope, key, value)
    });
    Some(entries.collect())
}

/// The `key=value` settings that replace each repo command setting: the user's own value of
/// that key, else a value that runs nothing of the repo's. None when a key cannot be passed
/// as `-c` (a `=` in its name).
fn overrides_of(listing: &str) -> Option<Vec<String>> {
    let entries = entries_of(listing)?;
    let mut out: Vec<String> = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    let mut helpers_done = false;
    for &(scope, key, _) in &entries {
        if trusted(scope) || done.contains(&key) {
            continue;
        }
        if key.contains('=') {
            return None;
        }
        done.push(key);
        if is_helper(key) {
            if !helpers_done {
                helpers_done = true;
                out.extend(helpers_again(&entries));
            }
        } else if key == "core.gitproxy" {
            // A list where the first match wins: a later `-c` cannot replace it.
            out.push(NO_GIT_PROTOCOL.to_string());
        } else {
            out.extend(replacement(&entries, key));
        }
    }
    Some(out)
}

fn is_helper(key: &str) -> bool {
    key.starts_with("credential.") && key.ends_with(".helper")
}

/// Helpers are a list, and an empty value clears it: clear, then the trusted ones in order.
fn helpers_again(entries: &[(&str, &str, &str)]) -> Vec<String> {
    let trusted_helpers = entries
        .iter()
        .filter(|(scope, key, _)| trusted(scope) && is_helper(key) && !key.contains('='))
        .map(|(_, key, value)| format!("{key}={value}"));
    std::iter::once("credential.helper=".to_string())
        .chain(trusted_helpers)
        .collect()
}

fn replacement(entries: &[(&str, &str, &str)], key: &str) -> Vec<String> {
    let own = entries
        .iter()
        .rev()
        .find(|(scope, each, _)| trusted(scope) && *each == key)
        .map(|(_, _, value)| value.to_string());
    let value = own.unwrap_or_else(|| fallback(key).to_string());
    let mut out = vec![format!("{key}={value}")];
    if let Some(driver) = key
        .strip_prefix("filter.")
        .and_then(|rest| rest.rsplit_once('.'))
        .map(|(driver, _)| driver)
        .filter(|_| value.is_empty())
    {
        // An empty filter is no filter; `required` would turn that into an error.
        out.push(format!("filter.{driver}.required=false"));
    }
    out
}

/// What a command setting falls back to: git's default program, or a value that runs nothing.
/// An empty `diff.external`, diff `command` or merge `driver` makes that diff or merge fail
/// ("cannot run"), which is safe.
fn fallback(key: &str) -> &'static str {
    let name = key.rsplit('.').next().unwrap_or(key);
    match (key.split('.').next().unwrap_or(key), name) {
        ("diff", "textconv") => "cat",
        ("core", "sshcommand") => "ssh",
        ("core", "pager") => "cat",
        ("core", "editor") | ("sequence", "editor") => ":",
        ("remote", "uploadpack") => "git-upload-pack",
        ("remote", "receivepack") => "git-receive-pack",
        ("gpg", "program") => match key {
            "gpg.ssh.program" => "ssh-keygen",
            "gpg.x509.program" => "gpgsm",
            _ => "gpg",
        },
        _ => "",
    }
}

#[cfg(test)]
#[path = "git_harden_tests.rs"]
mod tests;
