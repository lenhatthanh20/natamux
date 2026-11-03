//! The settings write of screen 5 and the custom agents, from the daemon or the fake. The read
//! of a project's settings is `load_settings`.

use natamux_client::Answer;
use natamux_proto::agents::custom::CustomAgent;
use natamux_proto::projects::repo::SavedSettings;
use serde_json::{Map, Value};

use super::Source;

impl Source {
    /// `PUT /repos/:id/settings`. `version` is what the read answered; the daemon refuses an
    /// old one with 409.
    pub fn save_settings(
        &self,
        repo_id: &str,
        raw: Map<String, Value>,
        version: Option<String>,
    ) -> Answer<SavedSettings> {
        let (fake_raw, fake_version) = (raw.clone(), version.clone());
        self.ask(
            |c| c.save_settings(repo_id, raw, version),
            |f| f.save_settings(repo_id, fake_raw, fake_version),
        )
    }

    /// `GET /custom-agents`.
    pub fn custom_agents(&self) -> Answer<Vec<CustomAgent>> {
        self.ask(|c| c.custom_agents(), |f| f.custom_agents())
    }

    /// `PUT /custom-agents`: the whole list; the daemon answers the saved one.
    pub fn save_custom_agents(&self, list: Vec<CustomAgent>) -> Answer<Vec<CustomAgent>> {
        let fake = list.clone();
        self.ask(
            |c| c.save_custom_agents(list),
            |f| f.save_custom_agents(fake),
        )
    }
}
