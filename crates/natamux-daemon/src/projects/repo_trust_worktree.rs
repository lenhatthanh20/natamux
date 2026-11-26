//! Trust in the task's worktree. An agent loads the config of the branch it runs on, so the
//! digest is taken there, not in the project folder. In Personal mode the project's approval
//! covers every worktree. In Strict mode approving one worktree adds its digest to the project's
//! approval; a later worktree with the same content is trusted with no new step.

use std::path::Path;
use std::sync::Arc;

use natamux_proto::projects::repo_trust::{CONFIG_CHANGED_REASON, RepoTrustView};

use crate::projects::errors::RepoError;
use crate::projects::repos::{DEFAULT_REPO, Repos};

/// What a launch or a command may rely on in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustState {
    Trusted {
        digest: String,
    },
    /// Not approved as it is now: the files that differ from the approval.
    Changed {
        digest: String,
        files: Vec<String>,
    },
    /// A link, a path that leaves the folder or a file over the caps: nothing can be approved.
    Unverifiable {
        reason: String,
    },
}

impl TrustState {
    pub fn is_trusted(&self) -> bool {
        matches!(self, TrustState::Trusted { .. })
    }

    pub fn digest(&self) -> Option<&str> {
        match self {
            TrustState::Trusted { digest } | TrustState::Changed { digest, .. } => Some(digest),
            TrustState::Unverifiable { .. } => None,
        }
    }

    /// Why an agent may not rely on the folder, for errors and logs.
    pub fn reason(&self) -> String {
        match self {
            TrustState::Trusted { .. } => String::new(),
            TrustState::Changed { files, .. } if files.is_empty() => CONFIG_CHANGED_REASON.into(),
            TrustState::Changed { files, .. } => {
                format!("{CONFIG_CHANGED_REASON} Changed: {}.", files.join(", "))
            }
            TrustState::Unverifiable { reason } => {
                format!("The agent config cannot be checked: {reason}.")
            }
        }
    }
}

/// The trust of a project (None: the default one) in a folder (None: the project folder), asked
/// at each use.
pub type TrustedOf = Arc<dyn Fn(Option<&str>, Option<&Path>) -> TrustState + Send + Sync>;

/// One answer for every project and folder: for tests and for runners built without repos.
pub fn trust_always(trusted: bool) -> TrustedOf {
    let state = if trusted {
        TrustState::Trusted {
            digest: "always".into(),
        }
    } else {
        TrustState::Changed {
            digest: "never".into(),
            files: Vec::new(),
        }
    };
    Arc::new(move |_: Option<&str>, _: Option<&Path>| state.clone())
}

fn changed_hash(id: &str) -> RepoError {
    RepoError::SettingsChanged(format!(
        "The commands of {id} changed since you saw them. Review them again."
    ))
}

impl Repos {
    /// The trust of a project in a task's worktree (None: the project folder).
    pub fn is_trusted_in_task(&self, repo: Option<&str>, worktree: Option<&Path>) -> TrustState {
        let id = repo.unwrap_or(DEFAULT_REPO);
        let Ok(current) = self.current(id, worktree) else {
            return TrustState::Unverifiable {
                reason: format!("project {id} is unknown"),
            };
        };
        let approvals = self.trust.approvals();
        let approval = approvals.get(id);
        // Personal mode trusts an approved project also where its agent config cannot be checked.
        if current.trusted(approval) {
            return TrustState::Trusted {
                digest: current.hash,
            };
        }
        if let Some(reason) = current.unverifiable.clone() {
            return TrustState::Unverifiable { reason };
        }
        TrustState::Changed {
            files: current.changed_files(approval),
            digest: current.hash,
        }
    }

    /// Whether the commands of a project may run, as its folder is now. An unknown project's
    /// may not.
    pub fn is_trusted(&self, repo: Option<&str>) -> bool {
        self.is_trusted_in_task(repo, None).is_trusted()
    }

    /// The setup script of a new worktree comes from its branch, which may hold another one than
    /// the project folder. It runs only when the worktree is approved.
    pub fn is_trusted_in(&self, repo: Option<&str>, worktree: &Path) -> bool {
        self.is_trusted_in_task(repo, Some(worktree)).is_trusted()
    }

    /// `GET /repos/:id/trust`: the commands as the user reviews them.
    pub fn trust_view(&self, id: &str) -> Result<RepoTrustView, RepoError> {
        self.trust_view_in(id, None)
    }

    /// `GET /repos/:id/trust?task=<id>`: the same, read in the task's worktree.
    pub fn trust_view_in(
        &self,
        id: &str,
        worktree: Option<&Path>,
    ) -> Result<RepoTrustView, RepoError> {
        let current = self.current(id, worktree)?;
        let approvals = self.trust.approvals();
        let approval = approvals.get(id);
        let trusted = current.trusted(approval);
        if worktree.is_none() {
            self.trust.note(id, trusted);
        }
        let changed = if trusted {
            Vec::new()
        } else {
            current.changed_files(approval)
        };
        let covered = current.covered(approval);
        Ok(current.view(trusted, covered, changed))
    }

    /// `POST /repos/:id/trust`: approves what the user saw. A hash that is no longer the current
    /// one is refused, so a change made meanwhile is reviewed too.
    pub fn approve(&self, id: &str, hash: &str) -> Result<RepoTrustView, RepoError> {
        self.approve_in(id, None, hash)
    }

    /// `POST /repos/:id/trust?task=<id>`: approves the digest of a task's worktree. It joins the
    /// project's accepted digests; the project folder's own approval stays.
    pub fn approve_in(
        &self,
        id: &str,
        worktree: Option<&Path>,
        hash: &str,
    ) -> Result<RepoTrustView, RepoError> {
        let current = self.current(id, worktree)?;
        if let Some(reason) = &current.unverifiable {
            return Err(RepoError::Input(format!(
                "The agent config cannot be checked, so it cannot be approved: {reason}"
            )));
        }
        if current.hash != hash {
            return Err(changed_hash(id));
        }
        let mut fresh = current.approval();
        fresh.by_user = true;
        self.trust.change(|all| match (worktree, all.get_mut(id)) {
            (Some(_), Some(approval)) if approval.path == fresh.path => {
                approval.accept(hash);
                approval.by_user = true;
            }
            _ => {
                all.insert(id.to_string(), fresh);
            }
        })?;
        if worktree.is_none() {
            self.trust.note(id, true);
        }
        Ok(current.view(true, true, Vec::new()))
    }
}

#[cfg(test)]
#[path = "repo_trust_worktree_tests.rs"]
mod tests;
