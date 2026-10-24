//! The files of the project list (`repo-service.ts`): repos.json, repos-removed.json and the
//! marker of the default project.

use std::path::Path;

use natamux_core::formats::REPO_ID;
use natamux_core::js::js_trim;
use natamux_core::json::{js_ordered, to_js_string_pretty};
use natamux_core::zod::{Issues, Pattern, Rule, ZodError, at, string};
use natamux_proto::projects::repo::Repo;
use serde_json::{Map, Value, json};

use crate::projects::errors::RepoError;
use crate::projects::repos::{DEFAULT_REPO, Repos};

/// One spelling of a folder: links followed, and case ignored where the file system ignores it. A
/// folder that is gone keeps its path.
pub fn canonical(path: &str) -> String {
    let real = if Path::new(path).exists() {
        std::fs::canonicalize(path).map_or_else(
            |_| path.to_string(),
            |real| real.to_string_lossy().into_owned(),
        )
    } else {
        path.to_string()
    };
    // Windows' long form starts with `\\?\`; Node's `realpath.native` has none.
    let real = real.strip_prefix(r"\\?\").unwrap_or(&real).to_string();
    if cfg!(any(windows, target_os = "macos")) {
        real.to_lowercase()
    } else {
        real
    }
}

/// `path.basename`: the last folder name, with a trailing slash ignored.
pub fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// `/^(\/|[A-Za-z]:[\\/]|\\\\)/`: a Unix, drive or UNC path.
fn is_absolute_text(text: &str) -> bool {
    let bytes = text.as_bytes();
    text.starts_with('/')
        || text.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

const WORKTREES_DIR: Rule = Rule::Regex(Pattern {
    test: is_absolute_text,
    source: "/^(\\/|[A-Za-z]:[\\\\/]|\\\\\\\\)/",
    message: Some("The worktree folder must be an absolute path"),
});

/// `repoSchema.parse` of one repository, which also trims the name.
pub(crate) fn check_repo(repo: &mut Repo, place: &[Value], issues: &mut Issues) {
    let id = at(place, "id");
    string(Some(&json!(repo.id)), &id, &REPO_ID, issues);
    let name = [Rule::Trim, Rule::Min(1), Rule::Max(100)];
    let named = string(Some(&json!(repo.name)), &at(place, "name"), &name, issues);
    if let Some(trimmed) = named {
        repo.name = trimmed;
    }
    let path = at(place, "path");
    string(Some(&json!(repo.path)), &path, &[Rule::Min(1)], issues);
    if let Some(dir) = &repo.worktrees_dir {
        let path = at(place, "worktreesDir");
        string(Some(&json!(dir)), &path, &[WORKTREES_DIR], issues);
    }
}

/// `z.array(repoSchema).parse`. Reading an older repos.json that has a relative worktree folder
/// fails here.
pub(crate) fn check_repos(repos: &mut [Repo]) -> Result<(), ZodError> {
    let mut issues = Issues::default();
    for (index, repo) in repos.iter_mut().enumerate() {
        check_repo(repo, &at(&[], index), &mut issues);
    }
    issues.into_result(Some(()))
}

/// `<file>.tmp` then a rename, so a crash leaves the old file whole.
fn write_whole(file: &Path, text: &str) -> Result<(), RepoError> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, file)?;
    Ok(())
}

impl Repos {
    /// Saves repos.json, and keeps the list as the cache.
    pub(crate) fn save_repos(&self, repos: &[Repo]) -> Result<(), RepoError> {
        let value =
            serde_json::to_value(repos).map_err(|error| RepoError::Failed(error.to_string()))?;
        write_whole(&self.file, &format!("{}\n", to_js_string_pretty(&value)))?;
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| RepoError::Failed("The project list failed earlier".into()))?;
        *cached = Some(repos.to_vec());
        Ok(())
    }

    /// The folder of each removed repository by id; empty when there is no file.
    pub(crate) fn removed(&self) -> Result<Map<String, Value>, RepoError> {
        let text = match std::fs::read_to_string(&self.removed_file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
            Err(error) => return Err(error.into()),
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) if map.values().all(Value::is_string) => Ok(map),
            Ok(_) => Err(RepoError::Failed(format!(
                "{} is not a list of folders by id",
                self.removed_file.display()
            ))),
            Err(error) => Err(RepoError::Failed(error.to_string())),
        }
    }

    pub(crate) fn save_removed(&self, paths: Map<String, Value>) -> Result<(), RepoError> {
        let text = to_js_string_pretty(&Value::Object(js_ordered(paths)));
        write_whole(&self.removed_file, &format!("{text}\n"))
    }

    /// The marker file keeps a removed repository from coming back at the next start.
    pub(crate) fn add_default_once(&self, path: &str) -> Result<(), RepoError> {
        if self.marker.exists() {
            return Ok(());
        }
        let stored = self.stored()?;
        let known = stored
            .iter()
            .any(|repo| repo.id == DEFAULT_REPO || canonical(&repo.path) == canonical(path));
        if !known {
            let base = basename(path);
            let name = if base.is_empty() {
                DEFAULT_REPO.to_string()
            } else {
                base
            };
            let mut all = vec![Repo {
                id: DEFAULT_REPO.into(),
                name,
                path: path.into(),
                worktrees_dir: None,
            }];
            all.extend(stored);
            self.save_repos(&all)?;
        }
        std::fs::write(&self.marker, format!("{path}\n"))?;
        Ok(())
    }

    /// Whether `folder` is the one that was added as "default".
    pub(crate) fn was_default(&self, folder: &str) -> bool {
        let Ok(text) = std::fs::read_to_string(&self.marker) else {
            return false;
        };
        let added = js_trim(&text);
        !added.is_empty() && canonical(added) == canonical(folder)
    }
}
