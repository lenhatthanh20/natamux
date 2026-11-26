//! The texts and choices of repo trust (`repo_trust_dialog.rs`): when the dialog opens, its title,
//! and the name it shows.

use natamux_proto::projects::repo::Repo;
use natamux_proto::projects::repo_trust::RepoTrustView;

/// Over the commands: why the dialog asks.
pub const LEAD: &str = "Natamux runs these on this machine for the tasks of this project, and \
                        keeps running them after they change. Strict mode asks again.";

/// After a 409: the commands changed after the dialog read them, so it shows the new ones.
pub const CHANGED: &str =
    "The commands changed since this dialog opened. Read the new ones, then approve again.";

/// The alert of the project settings page while the commands wait for approval.
pub const NOT_APPROVED_TITLE: &str = "Commands not approved";

pub const NOT_APPROVED: &str = "The commands of this project are not approved: stage checks, \
                                command stages and the setup script do not run, and agents start \
                                without auto mode.";

/// The alert of the project settings page for a project that runs, but was never approved.
pub const NOT_TRUSTED_TITLE: &str = "Not trusted yet";

pub const NOT_TRUSTED: &str = "Natamux asks once: trust this project, so its agents keep auto \
                               mode when its config changes later.";

/// The dialog of a project with nothing to list yet.
pub const NOTHING_YET: &str = "This project has no commands, setup script or agent config yet. \
                               Config that arrives later is trusted too.";

pub fn title(name: &str) -> String {
    format!("Trust {name}?")
}

/// The dialog opens when there is something to approve, and once per project the user never
/// approved, also with nothing to list: config that arrives later must not start an agent
/// without auto mode silently.
pub fn needs_review(view: &RepoTrustView) -> bool {
    let something = !view.commands.is_empty()
        || !view.setup.is_empty()
        || !view.agent_files.is_empty()
        || !view.changed.is_empty()
        || view.unverifiable.is_some();
    (!view.trusted && something) || !view.covered
}

/// Nothing to list: the dialog says so instead of an empty card.
pub fn nothing_listed(view: &RepoTrustView) -> bool {
    view.commands.is_empty() && view.setup.is_empty() && view.agent_files.is_empty()
}

/// Over the commands: what changed since the last approval, or why nothing can be approved.
pub fn changed_note(view: &RepoTrustView) -> Option<String> {
    if let Some(reason) = &view.unverifiable {
        return Some(format!("This folder cannot be approved: {reason}"));
    }
    if view.changed.is_empty() {
        return None;
    }
    Some(format!(
        "Changed since the last approval: {}",
        view.changed.join(", ")
    ))
}

/// The name the dialog shows; the id while the project list has not caught up with an add.
pub fn repo_name(repos: &[Repo], id: &str) -> String {
    repos
        .iter()
        .find(|repo| repo.id == id)
        .map_or_else(|| id.to_owned(), |repo| repo.name.clone())
}

#[cfg(test)]
#[path = "repo_trust_rules_tests.rs"]
mod tests;
