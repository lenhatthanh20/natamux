//! What a CI check may send to the remote: the `ciPush` key of Strict mode.

use super::{SecurityMode, mode};

/// `ciPush` of Strict mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CiPush {
    /// Only the last commit (HEAD), approved by the daemon.
    #[default]
    ApprovedCommit,
    /// HEAD plus the tracked changes, after a person approved the exact tree.
    ApprovedSnapshot,
    /// No CI push at all.
    Forbid,
}

/// How the CI check of a stage gets the work, in every mode. `Snapshot` is Personal mode: HEAD
/// plus the tracked changes, approved by the daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiMode {
    Snapshot,
    ApprovedCommit,
    ApprovedSnapshot,
    Forbid,
}

pub const CI_FORBID_REASON: &str =
    "Strict mode forbids CI pushes (ciPush: forbid), so this stage cannot be checked on CI.";

impl CiMode {
    pub fn of(mode: &SecurityMode) -> CiMode {
        match mode.strict().map(|settings| settings.ci_push) {
            None => CiMode::Snapshot,
            Some(CiPush::ApprovedCommit) => CiMode::ApprovedCommit,
            Some(CiPush::ApprovedSnapshot) => CiMode::ApprovedSnapshot,
            Some(CiPush::Forbid) => CiMode::Forbid,
        }
    }

    /// The name in events and settings.
    pub fn as_str(self) -> &'static str {
        match self {
            CiMode::Snapshot => "snapshot",
            CiMode::ApprovedCommit => "approved-commit",
            CiMode::ApprovedSnapshot => "approved-snapshot",
            CiMode::Forbid => "forbid",
        }
    }

    pub fn parse(text: &str) -> Option<CiMode> {
        [
            CiMode::Snapshot,
            CiMode::ApprovedCommit,
            CiMode::ApprovedSnapshot,
            CiMode::Forbid,
        ]
        .into_iter()
        .find(|each| each.as_str() == text)
    }
}

/// The CI mode now.
pub fn ci_mode() -> CiMode {
    CiMode::of(&mode())
}

#[cfg(test)]
mod tests {
    use super::super::{StrictSettings, force};
    use super::*;

    fn strict(ci_push: CiPush) -> SecurityMode {
        SecurityMode::Strict(StrictSettings {
            ci_push,
            ..StrictSettings::default()
        })
    }

    #[test]
    fn personal_sends_a_snapshot_and_strict_defaults_to_the_last_commit() {
        assert_eq!(ci_mode(), CiMode::Snapshot);
        let _strict = force(SecurityMode::Strict(StrictSettings::default()));
        assert_eq!(ci_mode(), CiMode::ApprovedCommit);
    }

    #[test]
    fn maps_each_ci_push_and_reads_its_name_back() {
        assert_eq!(
            CiMode::of(&strict(CiPush::ApprovedSnapshot)),
            CiMode::ApprovedSnapshot
        );
        assert_eq!(CiMode::of(&strict(CiPush::Forbid)), CiMode::Forbid);
        for mode in [
            CiMode::Snapshot,
            CiMode::ApprovedCommit,
            CiMode::ApprovedSnapshot,
            CiMode::Forbid,
        ] {
            assert_eq!(CiMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(CiMode::parse("all"), None);
    }
}
