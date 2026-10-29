//! The projects of the tasks (`repo-service.ts`, `project-settings.ts`): `<data>/repos.json` and
//! each repository's `.natamux.json`. Adding and removing projects are in `repo_add.rs`, their
//! files in `repo_store.rs`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use natamux_proto::projects::repo::Repo;
use natamux_proto::stages::schema::Step;

use crate::projects::errors::RepoError;
use crate::projects::repo_trust::TrustFile;
use crate::projects::settings::Cached;
use crate::shared::jobs::Clock;
use crate::tasks::errors::TaskError;

/// The id of the repository that tasks without one belong to.
pub const DEFAULT_REPO: &str = "default";

/// What `remove` needs of a task: its project and whether it has a workspace.
#[derive(Clone, Debug)]
pub struct TaskRef {
    pub repo: Option<String>,
    pub workspace: Option<String>,
}

/// The tasks of the daemon, read when a project is removed (Node's lazy `() => listTasks()`).
pub type TasksOf = Arc<dyn Fn() -> Vec<TaskRef> + Send + Sync>;

pub struct RepoOptions {
    pub data_dir: PathBuf,
    /// `NATAMUX_REPO_DIR`: added once as "default".
    pub default_path: Option<PathBuf>,
    /// The daemon's worktree folder; a project that names it keeps none of its own.
    pub worktrees_dir: Option<PathBuf>,
    pub tasks: Option<TasksOf>,
    pub now: Clock,
}

pub struct Repos {
    pub(crate) file: PathBuf,
    /// Holds the folder that was added as "default".
    pub(crate) marker: PathBuf,
    /// Holds the folder of each removed repository by id, as its tasks still name it.
    pub(crate) removed_file: PathBuf,
    pub(crate) worktrees_dir: Option<PathBuf>,
    // repos.json as last read: every hook asks for a path, so it is not read each time.
    pub(crate) cached: Mutex<Option<Vec<Repo>>>,
    /// One add or remove at a time: each reads the list and saves a changed copy.
    pub(crate) edit: Mutex<()>,
    pub(crate) tasks: OnceLock<TasksOf>,
    pub(crate) settings: Mutex<std::collections::HashMap<PathBuf, Cached>>,
    pub(crate) now: Clock,
    pub(crate) trust: TrustFile,
}

impl Repos {
    /// No default project and no daemon worktree folder: for tests of other parts.
    pub fn new(data_dir: &Path, now: Clock) -> Arc<Repos> {
        Repos::build(&RepoOptions {
            data_dir: data_dir.to_path_buf(),
            default_path: None,
            worktrees_dir: None,
            tasks: None,
            now,
        })
    }

    /// Adds `default_path` once as "default". Fails when repos.json cannot be read.
    pub fn with_options(options: RepoOptions) -> Result<Arc<Repos>, RepoError> {
        let repos = Repos::build(&options);
        if let Some(path) = &options.default_path {
            repos.add_default_once(&path.to_string_lossy())?;
        }
        repos.trust_first_start()?;
        Ok(repos)
    }

    fn build(options: &RepoOptions) -> Arc<Repos> {
        let tasks = OnceLock::new();
        if let Some(of) = &options.tasks {
            let _ = tasks.set(Arc::clone(of));
        }
        Arc::new(Repos {
            file: options.data_dir.join("repos.json"),
            marker: options.data_dir.join("repos-default-added"),
            removed_file: options.data_dir.join("repos-removed.json"),
            worktrees_dir: options.worktrees_dir.clone(),
            cached: Mutex::new(None),
            edit: Mutex::new(()),
            tasks,
            settings: Mutex::default(),
            now: Arc::clone(&options.now),
            trust: TrustFile::new(&options.data_dir),
        })
    }

    /// Set once, after the task service exists. Until then no task counts.
    pub fn set_tasks(&self, tasks: TasksOf) {
        let _ = self.tasks.set(tasks);
    }

    pub(crate) fn tasks_now(&self) -> Vec<TaskRef> {
        self.tasks.get().map_or_else(Vec::new, |of| of())
    }

    /// The daemon's folder for worktrees.
    pub fn worktrees_dir(&self) -> Option<&Path> {
        self.worktrees_dir.as_deref()
    }

    /// The projects in stored order.
    pub fn list(&self) -> Result<Vec<Repo>, RepoError> {
        Ok(self.stored()?)
    }

    pub(crate) fn stored(&self) -> Result<Vec<Repo>, TaskError> {
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| TaskError::Failed("The project list failed earlier".into()))?;
        if let Some(repos) = cached.as_ref() {
            return Ok(repos.clone());
        }
        let text = match std::fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "[]".into(),
            Err(error) => return Err(error.into()),
        };
        let mut repos: Vec<Repo> =
            serde_json::from_str(&text).map_err(|error| TaskError::Failed(error.to_string()))?;
        crate::projects::repo_store::check_repos(&mut repos)?;
        *cached = Some(repos.clone());
        Ok(repos)
    }

    /// None means the default repository.
    pub fn get(&self, id: Option<&str>) -> Result<Repo, TaskError> {
        let wanted = id.unwrap_or(DEFAULT_REPO);
        self.stored()?
            .into_iter()
            .find(|repo| repo.id == wanted)
            .ok_or_else(|| {
                TaskError::RepoNotFound(match id {
                    None => "There is no default project. Add a project first".into(),
                    Some(id) => format!("Repository {id} is not known. Add it first"),
                })
            })
    }

    pub fn path_of(&self, id: Option<&str>) -> Result<String, TaskError> {
        self.get(id).map(|repo| repo.path)
    }

    /// The project's own worktree folder; None means the daemon's (`worktrees_dir`). An unknown id
    /// gets None too: a removed project's tasks still need a place (`RepoService.worktreesDirOf`).
    pub fn worktrees_dir_of(&self, repo: Option<&str>) -> Option<PathBuf> {
        let wanted = repo.unwrap_or(DEFAULT_REPO);
        self.stored()
            .ok()?
            .into_iter()
            .find(|each| each.id == wanted)
            .and_then(|each| each.worktrees_dir)
            .map(PathBuf::from)
    }

    /// The stages of the project; the defaults for an unknown project or a file that is not
    /// valid, as Node's `get`.
    pub fn stages_of(&self, repo: Option<&str>) -> Vec<Step> {
        self.settings_of(repo).stages
    }
}
