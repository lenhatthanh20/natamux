//! What an isolated task shell may reach, and the variables it keeps. Pure apart from looking
//! at which paths exist, so tests can check every grant.

use std::path::{Path, PathBuf};

use natamux_core::security::IsolationSettings;
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
/// its own `lib` (an npm global install, `~/.cargo`), unless that parent is too wide.
pub fn toolchain_dirs(path: &str, home: &Path, data_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in std::env::split_paths(path) {
        if !entry.is_absolute() || !entry.is_dir() {
            continue;
        }
        let parent = entry
            .parent()
            .filter(|_| entry.file_name().is_some_and(|name| name == "bin"))
            .map(Path::to_path_buf);
        let chosen = parent
            .filter(|parent| !too_wide(parent, home, data_dir))
            .or_else(|| (!too_wide(&entry, home, data_dir)).then(|| entry.clone()));
        if let Some(dir) = chosen
            && !dirs.contains(&dir)
        {
            dirs.push(dir);
        }
    }
    dirs
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
    else {
        return (Vec::new(), Vec::new());
    };
    let shared = ["objects", "refs/heads", "logs/refs/heads"].map(|dir| common.join(dir));
    for dir in &shared {
        // Made here, outside the sandbox: the shared folder is read only inside it.
        let _ = std::fs::create_dir_all(dir);
        // A link inside the shared folder would carry the write grant somewhere else.
        if real(dir) != *dir {
            return (Vec::new(), Vec::new());
        }
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
    // Claude Code keeps its sign-in here: the one credential left in reach of the agent.
    read_write.extend(existing([home.join(".claude"), home.join(".claude.json")]));
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
