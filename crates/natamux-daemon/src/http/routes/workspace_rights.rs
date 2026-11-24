//! The push, pull request and issue routes check the agent's right again in the handler (F6),
//! so a mistake in the router rules alone does not open them.

use std::sync::Arc;

use hyper::{Method, StatusCode};
use natamux_core::security;

use super::{github, workspaces};
use crate::http::agent_actions::AgentAction;
use crate::http::agent_grants::AgentGrants;
use crate::http::agent_rules::{Caller, audit_denied_later, denial_line};
use crate::http::errors::{Answer, error_answer};
use crate::http::server::AppState;

pub async fn route(
    method: &Method,
    parts: &[&str],
    state: Arc<AppState>,
    who: &Caller,
) -> Option<Answer> {
    let (action, id) = match (method, parts) {
        (&Method::POST, ["tasks", id, "push"]) => (AgentAction::GitPushOwn, *id),
        (&Method::POST, ["tasks", id, "pull-request"]) => (AgentAction::PrCreateOwn, *id),
        (&Method::POST, ["tasks", id, "issue"]) => (AgentAction::IssueCreateOwn, *id),
        _ => return None,
    };
    if let Some(refused) = check(who, method, parts, action) {
        if let Caller::Agent(own) = who {
            audit_denied_later(&state, own).await;
        }
        return Some(refused);
    }
    let id = id.to_string();
    Some(match action {
        AgentAction::GitPushOwn => workspaces::push(state, id).await,
        AgentAction::PrCreateOwn => workspaces::open_pull_request(state, id).await,
        _ => github::create_issue(state, id).await,
    })
}

/// The 403 when the agent token may not do `action` on the task of `parts`: another task, or
/// a Strict grant it does not have. A person always may. `route` writes the audit line.
pub fn check(who: &Caller, method: &Method, parts: &[&str], action: AgentAction) -> Option<Answer> {
    let Caller::Agent(own) = who else {
        return None;
    };
    let on_own = matches!(parts, ["tasks", id, ..] if *id == own.as_str());
    if on_own && AgentGrants::of(&security::mode()).allows(action) {
        return None;
    }
    eprintln!("{}", denial_line(Some(action), method, parts, own));
    Some(error_answer(
        StatusCode::FORBIDDEN,
        "forbidden",
        &format!(
            "The agent token of this task has no {} grant.",
            action.name()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use natamux_core::security::{AgentApi, SecurityMode, StrictSettings, force};

    use super::*;

    fn strict(grants: &[&str]) -> SecurityMode {
        SecurityMode::Strict(StrictSettings {
            agent_api: AgentApi {
                grants: grants.iter().map(|name| name.to_string()).collect(),
                require_human_for_push: false,
                ..AgentApi::default()
            },
            ..StrictSettings::default()
        })
    }

    const PUSH: [&str; 3] = ["tasks", "T-1", "push"];

    #[test]
    fn a_person_and_a_personal_agent_on_its_own_task_pass() {
        let post = Method::POST;
        assert!(check(&Caller::User, &post, &PUSH, AgentAction::GitPushOwn).is_none());
        let own = Caller::Agent("T-1".into());
        assert!(check(&own, &post, &PUSH, AgentAction::GitPushOwn).is_none());
        let other = Caller::Agent("T-2".into());
        assert!(check(&other, &post, &PUSH, AgentAction::GitPushOwn).is_some());
    }

    #[test]
    fn strict_needs_the_grant_again_in_the_handler() {
        let own = Caller::Agent("T-1".into());
        let post = Method::POST;
        {
            let _strict = force(strict(&[]));
            let refused = check(&own, &post, &PUSH, AgentAction::GitPushOwn);
            assert_eq!(
                refused.map(|answer| answer.status()),
                Some(StatusCode::FORBIDDEN)
            );
            assert!(check(&Caller::User, &post, &PUSH, AgentAction::GitPushOwn).is_none());
        }
        let _strict = force(strict(&["git.push.own"]));
        assert!(check(&own, &post, &PUSH, AgentAction::GitPushOwn).is_none());
    }
}
