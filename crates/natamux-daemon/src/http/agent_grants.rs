//! The grants of an agent token in Strict mode (`agentApi.mode: scoped`): a fixed minimum, plus
//! the opt-in grants the user listed. Anything else answers 403.

use natamux_core::security::{AgentApi, SecurityMode};

use super::agent_actions::AgentAction;
use AgentAction::*;

/// What every agent needs to do its own work.
pub const MINIMUM: [AgentAction; 12] = [
    TaskReadOwn,
    TaskUpdateOwn,
    EventPostOwn,
    StageReportOwn,
    HookPost,
    LimitsPost,
    SubtaskCreate,
    SubtaskReadStatus,
    TerminalTicketOwn,
    FilesRwOwn,
    WorkspaceCommitOwn,
    AgentMessageOwn,
];

/// What `agentApi.grants` may add by name.
pub const OPT_IN: [AgentAction; 11] = [
    GitPushOwn,
    PrCreateOwn,
    IssueCreateOwn,
    AgentStartOwn,
    AgentReopenOwn,
    WorkspaceCreateOwn,
    RollbackOwn,
    SubtaskReadWorkspace,
    EventsSubscribe,
    RepoReadOwn,
    TasksList,
];

pub struct AgentGrants {
    /// None: not scoped, the Personal rules apply.
    granted: Option<Vec<AgentAction>>,
}

impl AgentGrants {
    pub fn of(mode: &SecurityMode) -> AgentGrants {
        AgentGrants {
            granted: mode
                .strict()
                .map(|strict| &strict.agent_api)
                .filter(|api| api.scoped)
                .map(granted),
        }
    }

    pub fn allows(&self, action: AgentAction) -> bool {
        self.granted
            .as_ref()
            .is_none_or(|granted| granted.contains(&action))
    }
}

fn granted(api: &AgentApi) -> Vec<AgentAction> {
    let opted = OPT_IN.into_iter().filter(|action| {
        let pushes = *action == GitPushOwn && api.require_human_for_push;
        !pushes && api.grants.iter().any(|name| name == action.name())
    });
    MINIMUM.into_iter().chain(opted).collect()
}

#[cfg(test)]
mod tests {
    use natamux_core::security::StrictSettings;

    use super::*;

    fn strict(grants: &[&str], require_human_for_push: bool) -> SecurityMode {
        SecurityMode::Strict(StrictSettings {
            agent_api: AgentApi {
                scoped: true,
                grants: grants.iter().map(|name| name.to_string()).collect(),
                require_human_for_push,
            },
            ..StrictSettings::default()
        })
    }

    #[test]
    fn personal_and_unscoped_strict_allow_every_action() {
        let unscoped = SecurityMode::Strict(StrictSettings {
            agent_api: AgentApi {
                scoped: false,
                ..AgentApi::default()
            },
            ..StrictSettings::default()
        });
        for mode in [SecurityMode::Personal, unscoped] {
            let grants = AgentGrants::of(&mode);
            for action in MINIMUM.into_iter().chain(OPT_IN).chain([GlobalRead]) {
                assert!(grants.allows(action), "{action:?}");
            }
        }
    }

    #[test]
    fn scoped_strict_allows_the_minimum_only_until_a_grant_is_listed() {
        let grants = AgentGrants::of(&strict(&[], true));
        for action in MINIMUM {
            assert!(grants.allows(action), "{action:?}");
        }
        for action in OPT_IN.into_iter().chain([GlobalRead]) {
            assert!(!grants.allows(action), "{action:?}");
        }
        let grants = AgentGrants::of(&strict(&["pr.create.own", "global.read", "nope"], true));
        assert!(grants.allows(PrCreateOwn));
        assert!(!grants.allows(IssueCreateOwn));
        // No grant opens the dashboard reads.
        assert!(!grants.allows(GlobalRead));
    }

    #[test]
    fn require_human_for_push_removes_the_push_grant() {
        let held = AgentGrants::of(&strict(&["git.push.own"], true));
        assert!(!held.allows(GitPushOwn));
        let given = AgentGrants::of(&strict(&["git.push.own"], false));
        assert!(given.allows(GitPushOwn));
    }
}
