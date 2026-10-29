//! What the updater does now, and its words for the status bar, Settings and the menu. Pure:
//! `update.rs` does the network and install work.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateState {
    /// No check ran yet, or a timed check failed (quietly).
    #[default]
    Idle,
    /// `manual`: the user asked, so the result shows.
    Checking {
        manual: bool,
    },
    Downloading {
        version: String,
        manual: bool,
    },
    Ready {
        version: String,
    },
    /// A newer release this install cannot apply itself.
    Manual {
        version: String,
        page: String,
    },
    UpToDate,
    /// Only after a manual check.
    Failed {
        message: String,
    },
}

/// What a check request does in the current state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStart {
    Run,
    /// A manual request while a check runs: the running check shows its result.
    Join,
    /// A manual request while an update is ready: show it again, no network.
    ShowReady,
    Skip,
}

/// What a click of the status item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Busy,
    Restart,
    OpenPage,
    Retry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusWords {
    pub kind: ItemKind,
    pub label: String,
    pub tooltip: Option<String>,
}

impl UpdateState {
    pub fn on_check(&self, manual: bool) -> CheckStart {
        match (self, manual) {
            (UpdateState::Checking { .. } | UpdateState::Downloading { .. }, true) => {
                CheckStart::Join
            }
            (UpdateState::Ready { .. }, true) => CheckStart::ShowReady,
            (
                UpdateState::Checking { .. }
                | UpdateState::Downloading { .. }
                | UpdateState::Ready { .. },
                false,
            ) => CheckStart::Skip,
            _ => CheckStart::Run,
        }
    }

    /// The same state, with a running check marked manual.
    pub fn joined(self) -> UpdateState {
        match self {
            UpdateState::Checking { .. } => UpdateState::Checking { manual: true },
            UpdateState::Downloading { version, .. } => UpdateState::Downloading {
                version,
                manual: true,
            },
            other => other,
        }
    }

    /// A check that found `version` starts its download; it keeps whether it is manual.
    pub fn downloading(self, version: &str) -> UpdateState {
        UpdateState::Downloading {
            version: version.to_owned(),
            manual: self.manual(),
        }
    }

    pub fn manual(&self) -> bool {
        matches!(
            self,
            UpdateState::Checking { manual: true } | UpdateState::Downloading { manual: true, .. }
        )
    }

    pub fn busy(&self) -> bool {
        matches!(
            self,
            UpdateState::Checking { .. } | UpdateState::Downloading { .. }
        )
    }

    /// The status bar item; None hides it. A timed check shows nothing until it downloads.
    pub fn status_item(&self, bytes: u64) -> Option<StatusWords> {
        let words = |kind, label: String, tooltip: Option<String>| {
            Some(StatusWords {
                kind,
                label,
                tooltip,
            })
        };
        match self {
            UpdateState::Checking { manual: true } => {
                words(ItemKind::Busy, "Checking for updates\u{2026}".into(), None)
            }
            UpdateState::Downloading { version, .. } => words(
                ItemKind::Busy,
                format!("Downloading {version} \u{b7} {}", megabytes(bytes)),
                Some(format!("Downloading Natamux {version}")),
            ),
            UpdateState::Ready { version } => words(
                ItemKind::Restart,
                "Restart to update".into(),
                Some(format!(
                    "Natamux {version} is ready. Restart to install it."
                )),
            ),
            UpdateState::Manual { version, .. } => words(
                ItemKind::OpenPage,
                format!("Natamux {version} available"),
                Some("Open the release page to install it".into()),
            ),
            UpdateState::Failed { message } => words(
                ItemKind::Retry,
                "Update failed".into(),
                Some(format!(
                    "{}. Click to try again.",
                    message.trim_end_matches('.')
                )),
            ),
            UpdateState::Checking { manual: false } | UpdateState::Idle | UpdateState::UpToDate => {
                None
            }
        }
    }

    /// The line under Version in Settings. `ago`: when the last check ended, as `ago` gives it.
    pub fn settings_line(&self, bytes: u64, ago: Option<&str>) -> String {
        match self {
            UpdateState::Idle => match ago {
                Some(ago) => format!("Last checked {ago}"),
                None => "Not checked yet".into(),
            },
            UpdateState::Checking { .. } => "Checking for updates\u{2026}".into(),
            UpdateState::Downloading { version, .. } => {
                format!("Downloading {version} \u{b7} {}", megabytes(bytes))
            }
            UpdateState::Ready { version } => format!("{version} is ready to install"),
            UpdateState::Manual { version, .. } => {
                format!("{version} is available on the release page")
            }
            UpdateState::UpToDate => match ago {
                Some(ago) => format!("Up to date \u{b7} last checked {ago}"),
                None => "Up to date".into(),
            },
            UpdateState::Failed { message } => format!("Last check failed: {message}"),
        }
    }

    /// The Settings button: its label, and whether it works now.
    pub fn check_button(&self) -> (&'static str, bool) {
        match self {
            UpdateState::Checking { .. } | UpdateState::Downloading { .. } => {
                ("Checking\u{2026}", false)
            }
            UpdateState::Ready { .. } => ("Restart to Update", true),
            _ => ("Check now\u{2026}", true),
        }
    }
}

/// Whole MB (MiB, as Finder and most apps count a download).
pub fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

/// How long ago, from elapsed seconds: just now, 4m ago, 3h ago, 2d ago.
pub fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86_399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

#[cfg(test)]
#[path = "update_state_tests.rs"]
mod tests;
