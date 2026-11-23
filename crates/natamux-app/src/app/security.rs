//! The security state the app shows (`GET /security`): read at connect and on
//! `security.changed`, and saved by Settings > Security (`PUT /security`).

use natamux_proto::projects::security::SecurityView;
use serde_json::Value;
use ui::prelude::*;

use super::super::Dashboard;
use crate::app::dashboard_data::Part;
use crate::components::toast_rules::ToastTone;

impl Dashboard {
    pub fn read_security(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let answer = source.security();
        cx.spawn(async move |this, cx| {
            let Ok(view) = answer.await else {
                return;
            };
            this.update(cx, |this, cx| this.use_security(view, cx)).ok();
        })
        .detach();
    }

    fn use_security(&mut self, view: SecurityView, cx: &mut Context<Self>) {
        if self.data.security != view {
            let warned = view.warning.is_some() && self.data.security.warning.is_none();
            if let Some(text) = view.warning.clone().filter(|_| warned) {
                self.toast(&text, ToastTone::Error, None, cx);
            }
            self.data.security = view;
            self.data.gens.bump(Part::Security);
            self.touch_view(cx);
        }
    }

    /// A save of Settings > Security; a refusal shows the daemon's words.
    pub fn save_security(&mut self, body: Value, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let answer = source.save_security(body);
        cx.spawn(async move |this, cx| {
            let saved = answer.await;
            this.update(cx, |this, cx| match saved {
                Ok(view) => this.use_security(view, cx),
                Err(error) => this.toast(&error.to_string(), ToastTone::Error, None, cx),
            })
            .ok();
        })
        .detach();
    }
}
