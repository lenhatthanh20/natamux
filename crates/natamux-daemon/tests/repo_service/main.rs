//! `repo-service.test.ts`, ported: one file per group. The Node test that checks the copies a
//! caller gets (`gives copies …`) only shows that a changed `Repo` does not change the list: Rust
//! returns owned values, so that is true by type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod add;
mod default_once;
mod remove;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use natamux_daemon::projects::repos::{RepoOptions, Repos, TaskRef, TasksOf};
use natamux_daemon::shared::jobs::system_clock;
use natamux_git::test_support::{TempDir, run_git, test_repo};
use serde_json::{Value, json};

pub fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A repository in a temporary folder, with `<root>/data` as the data folder and the repository
/// as the daemon's default project (`createTestRepo` and `beforeEach`).
pub struct Fx {
    pub root: TempDir,
    pub dir: PathBuf,
    pub repos: Arc<Repos>,
}

impl Fx {
    pub fn start() -> Fx {
        let (root, dir) = test_repo();
        let data = root.path().join("data");
        fs::create_dir(&data).unwrap();
        let repos = open(&data, Some(&dir), None, None);
        Fx { root, dir, repos }
    }

    pub fn data(&self) -> PathBuf {
        self.root.path().join("data")
    }

    /// A folder next to the repository, made a git repository (`gitFolder`).
    pub fn git_folder(&self, name: &str) -> PathBuf {
        git_init(&self.root.path().join(name))
    }
}

pub fn git_init(folder: &Path) -> PathBuf {
    fs::create_dir_all(folder).unwrap();
    run_git(folder, &["init", "-b", "main"]);
    folder.to_path_buf()
}

pub fn open(
    data: &Path,
    default: Option<&Path>,
    worktrees: Option<&Path>,
    tasks: Option<TasksOf>,
) -> Arc<Repos> {
    Repos::with_options(RepoOptions {
        data_dir: data.to_path_buf(),
        default_path: default.map(Path::to_path_buf),
        worktrees_dir: worktrees.map(Path::to_path_buf),
        tasks,
        now: system_clock(),
    })
    .unwrap()
}

pub fn ids(repos: &Repos) -> Vec<String> {
    repos
        .list()
        .unwrap()
        .into_iter()
        .map(|repo| repo.id)
        .collect()
}

pub fn input(path: &Path) -> Value {
    json!({ "path": text(path) })
}

pub fn task(repo: Option<&str>, workspace: Option<&str>) -> TaskRef {
    TaskRef {
        repo: repo.map(str::to_string),
        workspace: workspace.map(str::to_string),
    }
}

/// The tasks a test changes while the service runs (`tasks: () => tasks`).
pub type SharedTasks = Arc<Mutex<Vec<TaskRef>>>;

pub fn tasks_of(list: &SharedTasks) -> TasksOf {
    let list = Arc::clone(list);
    Arc::new(move || list.lock().unwrap().clone())
}

pub fn read_json(file: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}
