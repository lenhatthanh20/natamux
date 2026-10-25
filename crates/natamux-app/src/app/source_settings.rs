//! The settings write of screen 5, from the daemon or the fake. The read is `load_settings`.

use natamux_client::Answer;
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
}
