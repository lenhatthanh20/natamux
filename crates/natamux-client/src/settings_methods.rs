//! The `Client` calls of the settings: the project settings editor and Settings > Agents.

use natamux_proto::agents::custom::CustomAgent;
use natamux_proto::projects::repo::SavedSettings;
use serde_json::{Map, Value};

use crate::{Answer, Client, agents, projects};

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

    pub fn custom_agents(&self) -> Answer<Vec<CustomAgent>> {
        self.with_config(move |config| async move { agents::api::custom_agents(&config).await })
    }

    pub fn save_custom_agents(&self, list: Vec<CustomAgent>) -> Answer<Vec<CustomAgent>> {
        self.with_config(move |config| async move {
            agents::api::save_custom_agents(&config, list).await
        })
    }
}
