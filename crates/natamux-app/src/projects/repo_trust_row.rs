//! The `Review commands` alert of the project settings page: shown only while the project's
//! commands wait for approval, or the project waits for its one approval. It reads the trust
//! again when the projects change (an approval or a saved settings file sends `repos.changed`)
//! and after its dialog closes.

use std::rc::Rc;

use gpui::{Entity, Subscription};
use ui::prelude::*;

use super::rules::{
    NOT_APPROVED, NOT_APPROVED_TITLE, NOT_TRUSTED, NOT_TRUSTED_TITLE, needs_review,
};
use super::{Then, review};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Part, Seen};
use crate::components::button_style::{action_button, button_icon};
use crate::components::settings_tile::{AlertKind, SettingsAlert};

pub struct RepoTrustRow {
    dashboard: Entity<Dashboard>,
    repo_id: String,
    repo_name: String,
    untrusted: bool,
    /// Runs already, but was never approved: the row asks the one question, not for commands.
    runs: bool,
    _read: Option<gpui::Task<()>>,
    _follow: Subscription,
}

impl RepoTrustRow {
    pub fn new(
        dashboard: &Entity<Dashboard>,
        repo_id: &str,
        repo_name: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut seen = Seen::default();
        seen.take(&dashboard.read(cx).data.gens);
        let follow = Dashboard::observe(dashboard, cx, move |this, dashboard, cx| {
            let gens = dashboard.read(cx).data.gens;
            if seen.changed(&gens, &[Part::Repos]) {
                this.reload(cx);
            }
        });
        let mut row = RepoTrustRow {
            dashboard: dashboard.clone(),
            repo_id: repo_id.to_owned(),
            repo_name: repo_name.to_owned(),
            untrusted: false,
            runs: false,
            _read: None,
            _follow: follow,
        };
        row.reload(cx);
        row
    }

    /// The page shows the row only then, so a trusted project gets no empty gap.
    pub fn untrusted(&self) -> bool {
        self.untrusted
    }

    /// A failed read shows no row: the daemon still blocks the commands.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        // The project folder's own trust: no task.
        let answer = source.repo_trust(&self.repo_id, None);
        self._read = Some(cx.spawn(async move |this, cx| {
            let view = answer.await.ok();
            let untrusted = view.as_ref().is_some_and(needs_review);
            let runs = view.is_some_and(|view| view.trusted);
            this.update(cx, |this, cx| {
                this.untrusted = untrusted;
                this.runs = runs;
                cx.notify();
            })
            .ok();
        }));
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let this = cx.weak_entity();
        let then: Then = Rc::new(move |_: &mut Window, cx: &mut App| {
            this.update(cx, |this, cx| this.reload(cx)).ok();
        });
        review(
            &source,
            &self.repo_id,
            &self.repo_name,
            Some(then),
            window,
            cx,
        );
    }
}

impl Render for RepoTrustRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, text, button) = if self.runs {
            (NOT_TRUSTED_TITLE, NOT_TRUSTED, "Trust project")
        } else {
            (NOT_APPROVED_TITLE, NOT_APPROVED, "Review commands")
        };
        SettingsAlert::new(AlertKind::Warning, title)
            .icon(button_icon("shield"))
            .muted_text(text)
            .action(
                action_button("settings-review-commands", button)
                    .on_click(cx.listener(|this, _, window, cx| this.open(window, cx))),
            )
    }
}
