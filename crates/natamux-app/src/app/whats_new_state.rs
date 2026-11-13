//! The saved version of the last release notes (`whatsNewSeen` of `view.json`, T-0301).

use ui::prelude::*;

use super::Dashboard;

impl Dashboard {
    pub fn set_whats_new_seen(&mut self, version: &str, cx: &mut Context<Self>) {
        let version = version.to_owned();
        self.update_view(cx, |view| view.whats_new_seen = Some(version));
    }
}
