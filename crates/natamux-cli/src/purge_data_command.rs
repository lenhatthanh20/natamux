//! `natamux purge-data --yes`: removes Natamux's task data and logs from this machine: the task
//! files, events, screens, attachments, the index, the logs, the launch scripts, the sandbox
//! folders, and the app's token and port files. The worktrees are the user's code and stay;
//! settings (projects, security) stay too. Refused while a worktree has uncommitted work, while
//! the daemon runs (the app's saved port, or NATAMUX_PORT, else 4317), and in a task's terminal.
//! A second run finds nothing and changes nothing.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::args::Parsed;
use crate::run::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, Env, Io};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgeCommand {
    pub yes: bool,
}

/// Where the data is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataPaths {
    /// The app folder (token, port and app logs), None for a daemon run with NATAMUX_DATA_DIR.
    pub app: Option<PathBuf>,
    pub data: PathBuf,
    pub worktrees: PathBuf,
}

// The daemon's own default (natamux-daemon config); the CLI does not depend on that crate.
const DEFAULT_PORT: u16 = 4317;
const DATA_FOLDERS: [&str; 3] = ["tasks", "logs", "sandbox"];
const DATA_FILES: [&str; 3] = ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"];
const SCRIPT_PREFIXES: [&str; 2] = ["agent-launch-", "stage-command-"];
const APP_PATHS: [&str; 3] = ["logs", "daemon-token", "daemon-port"];

pub fn parse(words: &[String], values: &Parsed) -> Result<PurgeCommand, String> {
    if let Some(extra) = words.first() {
        return Err(format!("Unexpected argument: {extra}"));
    }
    Ok(PurgeCommand {
        yes: values.flag("yes"),
    })
}

/// NATAMUX_DATA_DIR (and NATAMUX_WORKTREES_DIR) as the daemon reads them, else the app's folder.
pub fn paths_of(env: &Env) -> Option<DataPaths> {
    let (app, data) = match env.trimmed("NATAMUX_DATA_DIR") {
        Some(data) => (None, PathBuf::from(data)),
        None => {
            let app = natamux_client::config::app_folder()?;
            let data = app.join("data");
            (Some(app), data)
        }
    };
    let worktrees = env
        .trimmed("NATAMUX_WORKTREES_DIR")
        .map_or_else(|| data.join("worktrees"), PathBuf::from);
    Some(DataPaths {
        app,
        data,
        worktrees,
    })
}

/// The paths a purge removes that exist now.
pub fn targets(paths: &DataPaths) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = DATA_FOLDERS
        .iter()
        .chain(DATA_FILES.iter())
        .map(|name| paths.data.join(name))
        .collect();
    let scripts = std::fs::read_dir(&paths.data)
        .into_iter()
        .flatten()
        .flatten();
    found.extend(
        scripts
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                SCRIPT_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            })
            .map(|entry| entry.path()),
    );
    if let Some(app) = &paths.app {
        found.extend(APP_PATHS.iter().map(|name| app.join(name)));
    }
    found.retain(|path| std::fs::symlink_metadata(path).is_ok());
    found.sort();
    found
}

/// Worktrees with work a purge would orphan: uncommitted or untracked files, or a status git
/// cannot read (that proves nothing).
pub fn dirty_worktrees(worktrees: &Path) -> Vec<PathBuf> {
    let mut dirty: Vec<PathBuf> = std::fs::read_dir(worktrees)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|folder| folder.join(".git").exists())
        .filter(|folder| {
            let status = Command::new("git")
                .arg("-C")
                .arg(folder)
                .args(["status", "--porcelain"])
                .output();
            !status.is_ok_and(|out| out.status.success() && out.stdout.is_empty())
        })
        .collect();
    dirty.sort();
    dirty
}

/// Removes the targets. A path that is gone already is fine.
pub fn purge(paths: &DataPaths) -> io::Result<Vec<PathBuf>> {
    let found = targets(paths);
    for path in &found {
        let removed = match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
            Ok(_) => std::fs::remove_file(path),
            Err(_) => Ok(()),
        };
        if let Err(error) = removed
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(error);
        }
    }
    Ok(found)
}

/// A daemon answers on the port the app saved, or on NATAMUX_PORT (else the default port) for a
/// daemon started by hand with NATAMUX_DATA_DIR.
fn daemon_runs(paths: &DataPaths, env: &Env) -> bool {
    let saved = paths.app.as_ref().and_then(|app| {
        std::fs::read_to_string(app.join("daemon-port"))
            .ok()
            .and_then(|text| text.trim().parse::<u16>().ok())
    });
    let own = env
        .trimmed("NATAMUX_PORT")
        .map_or(Some(DEFAULT_PORT), |raw| raw.parse::<u16>().ok());
    [saved, own]
        .into_iter()
        .flatten()
        .filter(|port| *port > 0)
        .any(|port| {
            let address = SocketAddr::from(([127, 0, 0, 1], port));
            TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
        })
}

fn list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format!("  {}\n", path.display()))
        .collect()
}

/// The checks and the purge, with what to print. Gives the exit code.
pub fn run_with(command: &PurgeCommand, paths: &DataPaths, env: &Env, io: &mut dyn Io) -> u8 {
    if env.trimmed("NATAMUX_TASK_ID").is_some() {
        io.stderr("purge-data runs only outside a task's terminal: a person runs it.\n");
        return EXIT_USAGE;
    }
    let dirty = dirty_worktrees(&paths.worktrees);
    if !dirty.is_empty() {
        io.stderr(&format!(
            "These worktrees have uncommitted work. Commit, push or remove it first:\n{}",
            list(&dirty)
        ));
        return EXIT_FAILED;
    }
    if daemon_runs(paths, env) {
        io.stderr("The Natamux daemon is running. Quit the app first.\n");
        return EXIT_FAILED;
    }
    let found = targets(paths);
    if found.is_empty() {
        io.stdout("Nothing to remove.\n");
        return EXIT_OK;
    }
    if !command.yes {
        io.stderr(&format!(
            "This removes:\n{}Worktrees stay. Run again with --yes to remove them.\n",
            list(&found)
        ));
        return EXIT_USAGE;
    }
    match purge(paths) {
        Ok(removed) => {
            io.stdout(&format!("Removed:\n{}", list(&removed)));
            EXIT_OK
        }
        Err(error) => {
            io.stderr(&format!("Error: {error}\n"));
            EXIT_FAILED
        }
    }
}

pub fn run(command: &PurgeCommand, env: &Env, io: &mut dyn Io) -> u8 {
    let Some(paths) = paths_of(env) else {
        io.stderr("The data folder is unknown: set NATAMUX_DATA_DIR.\n");
        return EXIT_USAGE;
    };
    run_with(command, &paths, env, io)
}
