//! What an isolated task shell may reach, and the variables it keeps. Apart from looking at
//! which paths exist, it only makes the shared git folders it grants and pins the checked git
//! folders for the daemon's own git (`pin_git_dirs`).

use std::path::{Path, PathBuf};

use natamux_core::security::{IsolationSettings, pin_git_dirs};
use natamux_pty::{Env, IsolationPlan};

/// The system folders every shell reads. Missing ones (`/lib32`) are left out.
const SYSTEM_READ_ONLY: [&str; 10] = [
    "/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/libx32", "/etc", "/proc", "/dev",
];
/// The devices a terminal writes to.
const DEVICES: [&str; 4] = ["/dev/null", "/dev/tty", "/dev/pts", "/dev/ptmx"];

/// The variables an isolated shell keeps, and the prefixes. Everything else (credentials such
/// as SSH_AUTH_SOCK or GH_TOKEN, trace switches) is dropped.
const KEEP: [&str; 23] = [
    "PATH",
    "LANG",
    "TERM",
    "SHELL",
    "USER",
    "LOGNAME",
    "HOME",
    "TMPDIR",
    "NATAMUX_URL",
    "NATAMUX_TASK_ID",
    "NATAMUX_TOKEN",
    "NATAMUX_TERMINAL",
    "NATAMUX_PORT",
    "PORT",
    "NATAMUX_BIN_DIR",
    // A company proxy is how Claude Code reaches its model at all.
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "all_proxy",
];
const KEEP_PREFIXES: [&str; 3] = ["LC_", "CLAUDE_", "ANTHROPIC_"];

/// The Claude Code settings with the hooks that report to the daemon (`start.rs`).
pub const CLAUDE_SETTINGS_FILE: &str = "claude-code-settings.json";
/// What Claude Code writes in `~/.claude` while it works. The daemon makes the missing folders
/// before a start, as a grant needs an existing path; it never makes the files.
pub const CLAUDE_WRITE_DIRS: [&str; 6] = [
    "projects",
    "todos",
    "shell-snapshots",
    "statsig",
    "session-env",
    "file-history",
];
const CLAUDE_WRITE_FILES: [&str; 2] = [".credentials.json", "history.jsonl"];

/// The variables of `env` that an isolated shell keeps: the allow-list and `allow`.
pub fn isolated_env(env: &Env, allow: &[String]) -> Env {
    env.iter()
        .filter(|(name, _)| {
            KEEP.contains(&name.as_str())
                || KEEP_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
                || allow.iter().any(|extra| extra == *name)
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

/// What a plan is made of.
pub struct PlanInput<'a> {
    pub worktree: &'a Path,
    /// `<data>/sandbox/<task>`: read only (launch scripts, git config).
    pub task_dir: &'a Path,
    /// `<task_dir>/tmp`: the shell's `TMPDIR`.
    pub tmp: &'a Path,
    pub home: &'a Path,
    pub data_dir: &'a Path,
    pub bin_dir: Option<&'a str>,
    /// The shell's PATH: its toolchain folders are read only.
    pub path: Option<&'a str>,
    pub settings: &'a IsolationSettings,
    pub sweep_mark: Option<String>,
}

fn existing(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|path| path.exists()).collect()
}

/// Whether granting `dir` would also grant something that must stay out: the home folder or the
/// data folder (daemon token, `security.json`), or a folder of the data folder.
fn too_wide(dir: &Path, home: &Path, data_dir: &Path) -> bool {
    dir.parent().is_none()
        || home.starts_with(dir)
        || data_dir.starts_with(dir)
        || dir.starts_with(data_dir)
}

/// The folders of a PATH, read only. A `.../bin` folder gives its parent, so a toolchain finds
/// its own `lib` (`/opt/tool`, an npm global install), unless that parent is too wide. Inside
/// home the parent may hold credentials (`~/.cargo/credentials.toml`): there only the folder and
/// its `lib` and `libexec` siblings.
pub fn toolchain_dirs(path: &str, home: &Path, data_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in std::env::split_paths(path) {
        if !entry.is_absolute() || !entry.is_dir() {
            continue;
        }
        let parent = entry
            .parent()
            .filter(|_| entry.file_name().is_some_and(|name| name == "bin"));
        let granted = match parent {
            Some(parent) if parent.starts_with(home) => {
                let mut granted = vec![entry.clone()];
                granted.extend(existing(["lib", "libexec"].map(|name| parent.join(name))));
                granted
            }
            Some(parent) if !too_wide(parent, home, data_dir) => vec![parent.to_path_buf()],
            _ => vec![entry.clone()],
        };
        for dir in granted {
            if !too_wide(&dir, home, data_dir) && !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// The folders of a git folder that a linked worktree's agent writes (see `git_grants`).
const GIT_WRITTEN: [&str; 4] = ["objects", "refs", "logs", "worktrees"];

/// Whether `common` lies in a folder that the agent of a worktree of another git folder may
/// write. The `.git` file is the agent's, so the grants of the real repo are not known here:
/// any git folder above `common` counts.
fn inside_git_writes(common: &Path) -> bool {
    common.ancestors().skip(1).any(|dir| {
        dir.file_name()
            .is_some_and(|name| GIT_WRITTEN.iter().any(|each| name == *each))
            && dir.parent().is_some_and(|git| git.join("HEAD").exists())
    })
}

/// A linked worktree writes its own git folder, the shared objects and branch refs; the rest of
/// the shared folder (config, hooks, info/exclude, other worktrees) is read only. A checkout
/// with its own `.git` inside is covered by the worktree grant.
fn git_grants(
    worktree: &Path,
    writable: &[PathBuf],
    home: &Path,
    data_dir: &Path,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    // The agent can rewrite `.git` and `commondir`, and build a fake shared folder where it can
    // write: only a checked shared folder outside its write grants counts, and never one that
    // would reach home or the data folder.
    let Some((git_dir, common)) = natamux_git::exec::linked_git_dirs(worktree)
        .map(|(git_dir, common)| (git_dir, real(&common)))
        .filter(|(_, common)| !too_wide(common, &real(home), &real(data_dir)))
        .filter(|(_, common)| !writable.iter().any(|dir| common.starts_with(real(dir))))
        .filter(|(_, common)| !inside_git_writes(common))
    else {
        pin_git_dirs(worktree, None);
        return (Vec::new(), Vec::new());
    };
    let shared = ["objects", "refs/heads", "logs/refs/heads"].map(|dir| common.join(dir));
    for dir in &shared {
        // Made here, outside the sandbox: the shared folder is read only inside it.
        let _ = std::fs::create_dir_all(dir);
        // A link inside the shared folder would carry the write grant somewhere else.
        if real(dir) != *dir {
            pin_git_dirs(worktree, None);
            return (Vec::new(), Vec::new());
        }
    }
    // The daemon's own git in this worktree uses the checked pair, never the `.git` file. A pair
    // that differs from the one pinned first gets nothing.
    if !pin_git_dirs(worktree, Some((git_dir.clone(), common.clone()))) {
        return (Vec::new(), Vec::new());
    }
    let mut write = vec![git_dir];
    write.extend(shared);
    (vec![common], existing(write))
}

pub fn plan_of(input: PlanInput<'_>) -> IsolationPlan {
    let PlanInput {
        worktree,
        task_dir,
        tmp,
        home,
        data_dir,
        bin_dir,
        path,
        settings,
        sweep_mark,
    } = input;
    let mut read_only = existing(SYSTEM_READ_ONLY.map(PathBuf::from));
    read_only.extend(toolchain_dirs(path.unwrap_or(""), home, data_dir));
    read_only.extend(existing(bin_dir.map(PathBuf::from)));
    read_only.extend(existing([data_dir.join(CLAUDE_SETTINGS_FILE)]));
    read_only.push(task_dir.to_path_buf());
    read_only.extend(existing(settings.extra_read_only.iter().cloned()));
    let mut read_write = existing(DEVICES.map(PathBuf::from));
    read_write.extend([worktree.to_path_buf(), tmp.to_path_buf()]);
    // Claude Code keeps its sign-in in `~/.claude`: the one credential left in reach of the
    // agent. Only what Claude Code writes while it works is writable, so the agent cannot plant
    // hooks, MCP servers or commands for the user's next Claude Code outside the sandbox. The
    // list is not tried with a real Claude Code.
    let claude = home.join(".claude");
    read_only.extend(existing([claude.clone(), home.join(".claude.json")]));
    let writes = CLAUDE_WRITE_DIRS
        .iter()
        .chain(&CLAUDE_WRITE_FILES)
        .map(|name| claude.join(name));
    // A link would carry the write grant out of `~/.claude`.
    read_write.extend(
        existing(writes)
            .into_iter()
            .filter(|path| !path.is_symlink()),
    );
    read_write.extend(existing(settings.extra_read_write.iter().cloned()));
    let (git_read, git_write) = git_grants(worktree, &read_write, home, data_dir);
    read_only.extend(git_read);
    read_write.extend(git_write);
    IsolationPlan {
        read_only,
        read_write,
        sweep_mark,
        max_abi: None,
        tcp_connect: None,
    }
}

#[cfg(test)]
#[path = "isolation_plan_tests.rs"]
mod tests;
