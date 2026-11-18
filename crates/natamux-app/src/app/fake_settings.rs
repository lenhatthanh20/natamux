//! The fake's settings files of screen 5: one `.natamux.json` per project with a version, and the
//! daemon's answers on them (read, save, a stale version, a pipeline that breaks the rules). The
//! raw keys other than `stages` are `fake_settings.json`; the stages are the reference pipeline.

use std::collections::BTreeMap;

use natamux_client::ClientError;
use natamux_proto::projects::repo::{LoadedSettings, ProjectSettings, SavedSettings};
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::{IssueLevel, pipeline_issues};
use serde_json::{Map, Value};

use super::{Answer, Data, FakeDaemon};

const SEED: &str = include_str!("fake_settings.json");

struct SettingsFile {
    raw: Map<String, Value>,
    version: u32,
}

/// The files by project id. A project with no entry has no file.
#[derive(Default)]
pub(super) struct SettingsFiles {
    files: BTreeMap<String, SettingsFile>,
}

fn path_of(repo_id: &str) -> String {
    format!("/data/projects/{repo_id}/.natamux.json")
}

fn bad_request(message: String) -> ClientError {
    ClientError::Status {
        status: 400,
        message,
    }
}

/// The settings of a raw file; a file with no `stages` key has the default pipeline.
fn parse(raw: &Map<String, Value>) -> Result<ProjectSettings, serde_json::Error> {
    let mut settings: ProjectSettings = serde_json::from_value(Value::Object(raw.clone()))?;
    if !raw.contains_key("stages") {
        settings.stages = default_pipeline();
    }
    Ok(settings)
}

impl SettingsFiles {
    fn seeded(natamux_stages: Value) -> SettingsFiles {
        let seed: Value = serde_json::from_str(SEED).unwrap_or(Value::Null);
        let mut files = BTreeMap::new();
        if let Some(Value::Object(mut raw)) = seed.get("natamux").cloned() {
            raw.insert("stages".into(), natamux_stages);
            files.insert("natamux".into(), SettingsFile { raw, version: 1 });
        }
        SettingsFiles { files }
    }

    /// `GET /repos/:id/settings`: the daemon's `version` of a missing file is "none".
    fn load(&self, repo_id: &str) -> LoadedSettings {
        let path = path_of(repo_id);
        let Some(file) = self.files.get(repo_id) else {
            let settings = ProjectSettings {
                stages: default_pipeline(),
                ..ProjectSettings::default()
            };
            return LoadedSettings {
                settings,
                path,
                version: Some("none".into()),
                ..LoadedSettings::default()
            };
        };
        let text = serde_json::to_string_pretty(&file.raw).map(|text| text + "\n");
        LoadedSettings {
            settings: parse(&file.raw).unwrap_or_default(),
            found: true,
            path,
            text: text.ok(),
            version: Some(format!("v{}", file.version)),
            ..LoadedSettings::default()
        }
    }

    /// `PUT /repos/:id/settings`: the daemon checks the shape first, then the version.
    fn save(
        &mut self,
        repo_id: &str,
        raw: Map<String, Value>,
        version: Option<String>,
    ) -> Answer<SavedSettings> {
        let settings = parse(&raw).map_err(|error| bad_request(format!("file: {error}")))?;
        let problems: Vec<String> = pipeline_issues(&settings.stages)
            .into_iter()
            .filter(|issue| issue.level == IssueLevel::Error)
            .map(|issue| issue.message)
            .collect();
        if !problems.is_empty() {
            return Err(bad_request(format!("stages: {}", problems.join("; "))));
        }
        let current = self.load(repo_id).version;
        if version.is_some() && version != current {
            return Err(ClientError::Status {
                status: 409,
                message: format!(
                    "{} changed since you opened it. Reload it, or overwrite it.",
                    path_of(repo_id)
                ),
            });
        }
        let next = self.files.get(repo_id).map_or(1, |file| file.version + 1);
        self.files
            .insert(repo_id.into(), SettingsFile { raw, version: next });
        Ok(SavedSettings {
            settings,
            version: self.load(repo_id).version,
        })
    }

    /// Another client saved: the version moves on and the text stays.
    fn move_version(&mut self, repo_id: &str) {
        if let Some(file) = self.files.get_mut(repo_id) {
            file.version += 1;
        }
    }
}

impl Data {
    /// The reference pipeline as the raw `stages` of the `natamux` file.
    pub(super) fn add_settings_seed(&mut self) {
        let stages = serde_json::to_value(super::stages::reference_pipeline());
        self.settings = SettingsFiles::seeded(stages.unwrap_or(Value::Null));
    }
}

impl FakeDaemon {
    pub fn load_settings(&self, repo_id: &str) -> Answer<LoadedSettings> {
        let data = self.data();
        let loaded = data.settings.load(repo_id);
        #[cfg(test)]
        if let Some(reason) = data.hooks.broken_settings.get(repo_id) {
            return Ok(LoadedSettings {
                error: Some(reason.clone()),
                text: None,
                ..loaded
            });
        }
        Ok(loaded)
    }

    pub fn save_settings(
        &self,
        repo_id: &str,
        raw: Map<String, Value>,
        version: Option<String>,
    ) -> Answer<SavedSettings> {
        self.data().settings.save(repo_id, raw, version)
    }

    /// For the conflict shot: the file changes as if another client had saved it.
    pub fn move_settings_version(&self, repo_id: &str) {
        self.data().settings.move_version(repo_id);
    }
}

#[cfg(test)]
#[path = "fake_settings_tests.rs"]
mod tests;
