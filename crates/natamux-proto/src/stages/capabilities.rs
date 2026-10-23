//! What a stage can do (`packages/core/src/stages/capabilities.ts`) and what it is called.

use crate::stages::catalog::catalog_stage;
use crate::stages::schema::{Stage, StageKind};

/// Agents that split a stage across sub-agents when the prompt asks for a workflow.
pub const WORKFLOW_AGENTS: [&str; 1] = ["claude-code"];

/// A task with no agent named gets Claude Code.
pub fn can_run_workflow(agent: Option<&str>) -> bool {
    WORKFLOW_AGENTS.contains(&agent.unwrap_or("claude-code"))
}

/// The project settings let tasks run this stage as a workflow.
pub fn offers_workflow(stage: &Stage) -> bool {
    stage.kind == StageKind::Agent && stage.workflow == Some(true)
}

/// CI checks code, so only a stage that changes it (an agent stage without an output file) or a
/// testing stage can wait for CI.
pub fn checks_on_ci(stage: &Stage) -> bool {
    stage.kind == StageKind::Testing || (stage.kind == StageKind::Agent && stage.output.is_none())
}

/// The project settings let tasks check this stage on CI.
pub fn offers_ci(stage: &Stage) -> bool {
    checks_on_ci(stage) && stage.ci == Some(true)
}

/// `stageName` for a stage that has no name of its own: the catalog name, else the id with spaces
/// and a capital ("test-spec" is "Test spec").
pub fn stage_name_of_id(id: &str) -> String {
    if let Some(name) = catalog_stage(id).and_then(|stage| stage.name) {
        return name;
    }
    let words = id.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

pub fn stage_name(stage: &Stage) -> String {
    stage
        .name
        .clone()
        .unwrap_or_else(|| stage_name_of_id(&stage.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::schema::Gate;

    fn named(id: &str, name: Option<&str>) -> Stage {
        let mut stage = Stage::new(id, StageKind::Agent, Gate::Manual);
        stage.name = name.map(str::to_owned);
        stage
    }

    fn of_kind(kind: StageKind) -> Stage {
        Stage::new("x", kind, Gate::Auto)
    }

    #[test]
    fn uses_the_name_of_the_catalog_for_a_catalog_stage() {
        assert_eq!(stage_name(&named("clarify", None)), "Open questions");
        assert_eq!(
            stage_name(&named("clarify", Some("Questions"))),
            "Questions"
        );
    }

    #[test]
    fn names_the_testing_and_review_stages_by_who_does_the_work() {
        for (id, name) in [
            ("testing", "Automated tests"),
            ("qa", "Manual testing"),
            ("agent-review", "Review"),
            ("review", "Manual review"),
        ] {
            assert_eq!(stage_name_of_id(id), name);
        }
    }

    #[test]
    fn uses_the_name_else_the_id_in_words() {
        assert_eq!(stage_name(&named("test-spec", None)), "Test spec");
        assert_eq!(
            stage_name(&named("build", Some("Test spec and Implement"))),
            "Test spec and Implement"
        );
        assert_eq!(stage_name_of_id(""), "");
    }

    #[test]
    fn is_offered_on_implement_and_automated_tests_of_the_default_pipeline() {
        let default = [
            "context",
            "spec",
            "plan",
            "test-spec",
            "implement",
            "testing",
            "agent-review",
            "release",
        ];
        let offered: Vec<&str> = default
            .into_iter()
            .filter(|id| offers_ci(&catalog_stage(id).unwrap()))
            .collect();
        assert_eq!(offered, ["implement", "testing"]);
    }

    #[test]
    fn offers_a_workflow_on_plan_and_implement_of_the_default_pipeline_only() {
        let offered: Vec<String> = crate::stages::catalog::catalog()
            .iter()
            .filter(|stage| offers_workflow(stage))
            .map(|stage| stage.id.clone())
            .collect();
        assert_eq!(offered, ["plan", "implement"]);
    }

    #[test]
    fn offers_a_workflow_on_a_stage_of_your_own_that_asks_for_it_and_not_without_the_field() {
        let mut stage = of_kind(StageKind::Agent);
        assert!(!offers_workflow(&stage));
        stage.workflow = Some(false);
        assert!(!offers_workflow(&stage));
        stage.workflow = Some(true);
        assert!(offers_workflow(&stage));
        let mut review = of_kind(StageKind::Review);
        review.workflow = Some(true);
        assert!(!offers_workflow(&review));
    }

    #[test]
    fn lets_only_claude_code_run_a_workflow_a_task_with_no_agent_gets_claude_code() {
        assert!(can_run_workflow(Some("claude-code")));
        assert!(can_run_workflow(None));
        for agent in ["codex", "gemini", "copilot", "cursor"] {
            assert!(!can_run_workflow(Some(agent)));
        }
    }

    #[test]
    fn is_allowed_only_on_a_stage_that_changes_code_or_runs_tests() {
        assert!(checks_on_ci(&of_kind(StageKind::Agent)));
        assert!(checks_on_ci(&of_kind(StageKind::Testing)));
        let mut notes = of_kind(StageKind::Agent);
        notes.output = Some("notes.md".into());
        assert!(!checks_on_ci(&notes));
        assert!(!checks_on_ci(&of_kind(StageKind::Review)));
        assert!(!checks_on_ci(&of_kind(StageKind::Manual)));
    }
}
