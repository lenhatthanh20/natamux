//! The `Client` calls of the settings: the project settings editor and Settings > Agents.

use hyper::Method;
use natamux_proto::agents::custom::CustomAgent;
use natamux_proto::projects::repo::SavedSettings;
use natamux_proto::projects::security::SecurityView;
use serde_json::{Map, Value};

use crate::http::{call, call_with};
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

    /// `GET /security`: the security mode and what each key blocks.
    pub fn security(&self) -> Answer<SecurityView> {
        self.with_config(move |config| async move { call(&config, Method::GET, "/security").await })
    }

    /// `PUT /security`: the body is the shape of `security.json`; the answer is the new state.
    pub fn save_security(&self, body: Value) -> Answer<SecurityView> {
        self.with_config(move |config| async move {
            call_with(&config, Method::PUT, "/security", &body).await
        })
    }
}
