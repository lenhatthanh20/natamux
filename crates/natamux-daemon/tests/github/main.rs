//! `issue-importer.test.ts`, `issue-linker.test.ts` and `pull-request-watcher.test.ts`, ported.
//! Node's `vi.useFakeTimers` is the manual `Scheduler`; its async waits are `settle` of the
//! service. A stub `RunGh` records the folder and the arguments of each call, as Node's did.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod issue_importer;
mod issue_linker;
mod pull_request_branch_sync;
mod pull_request_finding;
mod pull_request_watcher;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::jobs::{Jobs, system_clock};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::stages::run::{RepoPath, SettingsOf};
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::tasks::store::SettingsOf as StagesOf;
use natamux_git::gh::{GitHubError, RunGh};
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_store::task_index::TaskIndex;

/// A fresh folder for one test, removed when the test ends.
pub struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Debug)]
pub struct GhCall {
    pub cwd: String,
    pub args: Vec<String>,
}

pub type Calls = Arc<Mutex<Vec<GhCall>>>;

/// The values that follow `flag` in the arguments of a gh call.
pub fn values_of(call: &GhCall, flag: &str) -> Vec<String> {
    call.args
        .iter()
        .enumerate()
        .filter(|(at, _)| *at > 0 && call.args[*at - 1] == flag)
        .map(|(_, value)| value.clone())
        .collect()
}

/// A gh that records each call, then answers with `answer`.
pub fn recording(
    answer: impl Fn(&GhCall) -> Result<String, GitHubError> + Send + Sync + 'static,
) -> (RunGh, Calls) {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let gh: RunGh = Arc::new(move |cwd: &Path, args: &[String]| {
        let call = GhCall {
            cwd: cwd.to_string_lossy().into_owned(),
            args: args.to_vec(),
        };
        seen.lock().unwrap().push(call.clone());
        answer(&call)
    });
    (gh, calls)
}

pub fn jobs() -> Arc<Jobs> {
    Arc::new(Jobs::new(None, system_clock()))
}

pub fn repo_path() -> RepoPath {
    Arc::new(|_| Ok("/repo".to_string()))
}

pub struct Fx {
    pub dir: TempDir,
    /// The bus of the task service.
    pub bus: Arc<EventBus>,
    pub tasks: TaskService,
    pub scheduler: Arc<Scheduler>,
    /// What `.natamux.json` says; a test changes it while the services run.
    pub settings: Arc<Mutex<ProjectSettings>>,
}

impl Default for Fx {
    fn default() -> Fx {
        Fx::new()
    }
}

impl Fx {
    pub fn new() -> Fx {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("natamux-github-{}-{count}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = Arc::new(Mutex::new(ProjectSettings {
            stages: default_pipeline(),
            ..ProjectSettings::default()
        }));
        let read = Arc::clone(&settings);
        let stages_of: StagesOf = Arc::new(move |_| read.lock().unwrap().stages.clone());
        let bus = EventBus::new();
        let tasks = TaskService::new(TaskServiceOptions {
            data_dir: dir.clone(),
            index: TaskIndex::open(Path::new(":memory:")).unwrap(),
            bus: Arc::clone(&bus),
            now: None,
            settings_of: Some(stages_of),
            clock: None,
        });
        Fx {
            dir: TempDir(dir),
            bus,
            tasks,
            scheduler: Scheduler::manual(),
            settings,
        }
    }

    pub fn settings_of(&self) -> SettingsOf {
        let read = Arc::clone(&self.settings);
        Arc::new(move |_| read.lock().unwrap().clone())
    }
}
