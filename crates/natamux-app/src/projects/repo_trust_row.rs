//! The `Review commands` row of the project settings page: shown only while the project's commands
//! wait for approval. It reads the trust again when the projects change (an approval or a saved
//! settings file sends `repos.changed`) and after its dialog closes.

use std::rc::Rc;

use gpui::{Entity, Subscription};
use ui::prelude::*;

use super::rules::{NOT_APPROVED, needs_review};
use super::{Then, review};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Part, Seen};
use crate::components::button_style::action_button;

pub struct RepoTrustRow {
    dashboard: Entity<Dashboard>,
    repo_id: String,
    repo_name: String,
    untrusted: bool,
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
        let follow = cx.observe(dashboard, move |this, dashboard, cx| {
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
        let answer = source.repo_trust(&self.repo_id);
        self._read = Some(cx.spawn(async move |this, cx| {
            let untrusted = answer.await.is_ok_and(|view| needs_review(&view));
            this.update(cx, |this, cx| {
                this.untrusted = untrusted;
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
        let status = cx.theme().status();
        h_flex()
            .flex_wrap()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(status.warning_border)
            .bg(status.warning_background)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(Label::new(NOT_APPROVED).color(Color::Default)),
            )
            .child(
                action_button("settings-review-commands", "Review commands")
                    .on_click(cx.listener(|this, _, window, cx| this.open(window, cx))),
            )
    }
}
