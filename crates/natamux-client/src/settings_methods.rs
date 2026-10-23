//! The `Client` call of the project settings editor.

use natamux_proto::projects::repo::SavedSettings;
use serde_json::{Map, Value};

use crate::{Answer, Client, projects};

impl Client {
    /// `version`: from `load_settings`; None writes over whatever is on disk.
    pub fn save_settings(
        &self,
        repo_id: &str,
        raw: Map<String, Value>,
        version: Option<String>,
    ) -> Answer<SavedSettings> {
        let repo_id = repo_id.to_string();
        self.with_config(move |config| async move {
            projects::api::save_settings(&config, &repo_id, &raw, version.as_deref()).await
        })
    }
}
