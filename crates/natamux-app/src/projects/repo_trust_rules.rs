//! The texts and choices of repo trust (`repo_trust_dialog.rs`): when the dialog opens, its title,
//! and the name it shows.

use natamux_proto::projects::repo::Repo;
use natamux_proto::projects::repo_trust::RepoTrustView;

/// Over the commands: why the dialog asks.
pub const LEAD: &str = "Natamux runs these on this machine for the tasks of this project.";

/// After a 409: the commands changed after the dialog read them, so it shows the new ones.
pub const CHANGED: &str =
    "The commands changed since this dialog opened. Read the new ones, then approve again.";

/// The row of the project settings page while the commands wait for approval.
pub const NOT_APPROVED: &str = "The commands of this project are not approved: stage checks, \
                                command stages and the setup script do not run, and agents start \
                                without auto mode.";

pub fn title(name: &str) -> String {
    format!("Approve the commands of {name}?")
}

/// The dialog opens only when there is something to approve; the daemon approves a project with
/// no commands on its own.
pub fn needs_review(view: &RepoTrustView) -> bool {
    !view.trusted && (!view.commands.is_empty() || !view.setup.is_empty())
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
