//! The projects of the tasks, read only (`repo-service.ts`, `project-settings.ts`):
//! `<data>/repos.json` and each repository's `.natamux.json`. Adding and removing projects, and
//! the settings routes, come with a later part.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use natamux_proto::projects::repo::{ProjectSettings, Repo};
use natamux_proto::projects::settings_rules::other_settings_problem;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::{IssueLevel, pipeline_issues};
use natamux_proto::stages::schema::Step;
use serde_json::Value;

use crate::shared::jobs::Clock;
use crate::tasks::errors::TaskError;

/// The id of the repository that tasks without one belong to.
pub const DEFAULT_REPO: &str = "default";
/// How long a look at a settings file counts.
const STAT_EVERY_MS: i64 = 1000;

struct Cached {
    mtime: Option<SystemTime>,
    checked_at: i64,
    settings: ProjectSettings,
}

pub struct Repos {
    file: PathBuf,
    // repos.json as last read: every hook asks for a path, so it is not read each time.
    cached: Mutex<Option<Vec<Repo>>>,
    settings: Mutex<std::collections::HashMap<PathBuf, Cached>>,
    now: Clock,
}

impl Repos {
    pub fn new(data_dir: &Path, now: Clock) -> Arc<Repos> {
        Arc::new(Repos {
            file: data_dir.join("repos.json"),
            cached: Mutex::new(None),
            settings: Mutex::default(),
            now,
        })
    }

    fn stored(&self) -> Result<Vec<Repo>, TaskError> {
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
        let repos: Vec<Repo> =
            serde_json::from_str(&text).map_err(|error| TaskError::Failed(error.to_string()))?;
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

    /// The stages of the project; the defaults for an unknown project or a file that is not
    /// valid, as Node's `get`.
    pub fn stages_of(&self, repo: Option<&str>) -> Vec<Step> {
        self.settings_of(repo).stages
    }

    /// The settings of the project, with the default pipeline when the file has no stages. The
    /// defaults for an unknown project or a file that is not valid. shortcut: a file is valid when
    /// proto reads it and its pipeline has no error; the zod texts of a broken file come with the
    /// settings routes.
    pub fn settings_of(&self, repo: Option<&str>) -> ProjectSettings {
        let Ok(path) = self.path_of(repo) else {
            return default_settings();
        };
        let file = Path::new(&path).join(".natamux.json");
        let now = (self.now)();
        let Ok(mut cache) = self.settings.lock() else {
            return default_settings();
        };
        if let Some(known) = cache.get(&file)
            && now - known.checked_at < STAT_EVERY_MS
        {
            return known.settings.clone();
        }
        let mtime = std::fs::metadata(&file)
            .and_then(|meta| meta.modified())
            .ok();
        let settings = match cache.get(&file) {
            Some(known) if known.mtime == mtime => known.settings.clone(),
            _ => read_settings(&file, mtime.is_some()),
        };
        cache.insert(
            file,
            Cached {
                mtime,
                checked_at: now,
                settings: settings.clone(),
            },
        );
        settings
    }
}

fn default_settings() -> ProjectSettings {
    ProjectSettings {
        stages: default_pipeline(),
        ..ProjectSettings::default()
    }
}

fn read_settings(file: &Path, found: bool) -> ProjectSettings {
    if !found {
        return default_settings();
    }
    let parsed = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let Some(Value::Object(raw)) = parsed else {
        return default_settings();
    };
    if other_settings_problem(&raw).is_some() {
        return default_settings();
    }
    let Ok(mut settings) = serde_json::from_value::<ProjectSettings>(Value::Object(raw)) else {
        return default_settings();
    };
    crate::projects::settings_trim::trim_settings(&mut settings);
    if settings.stages.is_empty() {
        return ProjectSettings {
            stages: default_pipeline(),
            ..settings
        };
    }
    if pipeline_issues(&settings.stages)
        .iter()
        .any(|issue| issue.level == IssueLevel::Error)
    {
        return default_settings();
    }
    settings
}
