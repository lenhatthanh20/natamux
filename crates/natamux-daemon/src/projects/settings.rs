//! `.natamux.json` of each project (`project-settings.ts`): read again when the file changes, so an
//! edit in the editor counts within a second, and saved with a version check.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use natamux_core::json::to_js_string_pretty;
use natamux_proto::projects::repo::ProjectSettings;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::projects::errors::RepoError;
use crate::projects::repos::Repos;
use crate::projects::settings_parse::{
    SETTINGS_FILE, default_settings, parse_settings, parse_text, settings_json,
};
use crate::tasks::errors::TaskError;

/// How long a look at a settings file counts. Hooks ask for the settings on every tool call.
const STAT_EVERY_MS: i64 = 1000;
/// The version of a missing file.
const NO_FILE: &str = "none";

/// What a look at the file found (`LoadedSettings`). `json` is `settings` in zod's key order.
#[derive(Clone)]
pub(crate) struct Loaded {
    settings: ProjectSettings,
    json: Value,
    error: Option<String>,
    found: bool,
    text: Option<String>,
    version: String,
}

pub(crate) struct Cached {
    mtime: Option<SystemTime>,
    checked_at: i64,
    loaded: Loaded,
}

/// `versionOf`: "none" for no file, else the first 16 hex digits of the SHA-256 of the text.
pub fn version_of(text: Option<&str>) -> String {
    let Some(text) = text else {
        return NO_FILE.into();
    };
    let digest = Sha256::digest(text.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn not_found() -> Loaded {
    let settings = default_settings();
    Loaded {
        json: settings_json(&settings),
        settings,
        error: None,
        found: false,
        text: None,
        version: NO_FILE.into(),
    }
}

fn read_file(file: &Path, found: bool) -> Loaded {
    if !found {
        return not_found();
    }
    let text = std::fs::read_to_string(file).ok();
    let version = version_of(text.as_deref());
    let (settings, json, error) = match parse_text(text.as_deref().unwrap_or("")) {
        Ok(parsed) => (parsed.settings, parsed.json, None),
        Err(error) => {
            let settings = default_settings();
            (settings.clone(), settings_json(&settings), Some(error))
        }
    };
    Loaded {
        settings,
        json,
        error,
        found: true,
        text,
        version,
    }
}

fn repo_error(error: TaskError) -> RepoError {
    match error {
        TaskError::RepoNotFound(text) => RepoError::NotFound(text),
        other => RepoError::Failed(other.to_string()),
    }
}

impl Repos {
    fn settings_file(&self, id: Option<&str>) -> Result<PathBuf, TaskError> {
        Ok(Path::new(&self.path_of(id)?).join(SETTINGS_FILE))
    }

    /// `load` with the one-second look cache.
    fn loaded(&self, repo: Option<&str>) -> Result<Loaded, TaskError> {
        let file = self.settings_file(repo)?;
        let now = (self.now)();
        let mut cache = self
            .settings
            .lock()
            .map_err(|_| TaskError::Failed("The settings cache failed earlier".into()))?;
        if let Some(known) = cache.get(&file)
            && now - known.checked_at < STAT_EVERY_MS
        {
            return Ok(known.loaded.clone());
        }
        let mtime = std::fs::metadata(&file)
            .and_then(|meta| meta.modified())
            .ok();
        let loaded = match cache.get(&file) {
            Some(known) if known.mtime == mtime => known.loaded.clone(),
            _ => read_file(&file, mtime.is_some()),
        };
        cache.insert(
            file,
            Cached {
                mtime,
                checked_at: now,
                loaded: loaded.clone(),
            },
        );
        Ok(loaded)
    }

    /// The settings of the project, with the default pipeline when the file has no stages. The
    /// defaults for an unknown project or a file that cannot be used (`ProjectSettingsService.get`).
    pub fn settings_of(&self, repo: Option<&str>) -> ProjectSettings {
        self.loaded(repo)
            .map_or_else(|_| default_settings(), |loaded| loaded.settings)
    }

    /// `GET /repos/:id/settings`: `{settings, error, found, text, version, path}`.
    pub fn load(&self, id: &str) -> Result<Value, RepoError> {
        let path = self.settings_file(Some(id)).map_err(repo_error)?;
        let loaded = self.loaded(Some(id)).map_err(repo_error)?;
        Ok(json!({
            "settings": loaded.json,
            "error": loaded.error,
            "found": loaded.found,
            "text": loaded.text,
            "version": loaded.version,
            "path": path.display().to_string(),
        }))
    }

    /// `PUT /repos/:id/settings`: writes `body` as it is. With `version`, the file must not have
    /// changed since it was read. Gives the settings as zod reads them, and the new version.
    pub fn save(
        &self,
        id: &str,
        body: &Value,
        version: Option<&str>,
    ) -> Result<(Value, String), RepoError> {
        let parsed = parse_settings(body).map_err(RepoError::SettingsInput)?;
        let path = self.settings_file(Some(id)).map_err(repo_error)?;
        if let Some(version) = version
            && version_of(std::fs::read_to_string(&path).ok().as_deref()) != version
        {
            return Err(RepoError::SettingsChanged(format!(
                "{} changed since you opened it. Reload it, or overwrite it.",
                path.display()
            )));
        }
        // `JSON.stringify(raw, null, 2)`: `1.0` as `1`, keys like "1" first, as Node writes them.
        let text = format!("{}\n", to_js_string_pretty(body));
        let mut temporary = path.clone().into_os_string();
        temporary.push(".tmp");
        std::fs::write(&temporary, &text)
            .and_then(|()| std::fs::rename(&temporary, &path))
            .map_err(|error| RepoError::Failed(error.to_string()))?;
        if let Ok(mut cache) = self.settings.lock() {
            cache.remove(&path);
        }
        Ok((parsed.json, version_of(Some(&text))))
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
