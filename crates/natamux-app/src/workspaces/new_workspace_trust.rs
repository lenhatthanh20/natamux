//! Start a task in a project whose commands wait for approval: the repo trust dialog opens first,
//! and the start goes on after either answer. Not approved, the daemon runs none of the project's
//! commands, so the task still starts safely.

use std::rc::Rc;

use ui::prelude::*;

use super::super::new_workspace_rules::blocked_reason;
use super::NewWorkspaceDialog;
use crate::projects::repo_trust_dialog::rules::repo_name;
use crate::projects::repo_trust_dialog::{Then, review};

impl NewWorkspaceDialog {
    pub(super) fn start_checked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || blocked_reason(&self.form(cx), &self.choice()).is_some() {
            return;
        }
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let repo = self.repo(cx);
        let name = repo_name(&self.dashboard.read(cx).data.repos, &repo);
        let this = cx.weak_entity();
        let then: Then = Rc::new(move |_: &mut Window, cx: &mut App| {
            this.update(cx, |this, cx| this.start(cx)).ok();
        });
        review(&source, &repo, &name, Some(then), window, cx);
    }
}
