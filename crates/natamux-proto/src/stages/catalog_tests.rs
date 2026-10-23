use super::*;
use crate::stages::catalog_text::TEXTS;
use crate::stages::field_rules::stage_problems;
use crate::stages::prompt::default_prompt;
use crate::stages::schema::GROUP_KINDS;

fn ids_of(phase: StagePhase) -> Vec<&'static str> {
    catalog_entries()
        .iter()
        .filter(|entry| entry.phase == phase)
        .map(|entry| entry.stage.id.as_str())
        .collect()
}

#[test]
fn has_every_stage_once() {
    let ids: Vec<String> = catalog().iter().map(|stage| stage.id.clone()).collect();
    assert_eq!(ids.len(), 28);
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
    let texts: Vec<&str> = TEXTS.iter().map(|text| text.id).collect();
    assert_eq!(texts, ids);
}

#[test]
fn finds_a_stage_by_id() {
    let spec = catalog_stage("spec").unwrap();
    assert_eq!(spec.output.as_deref(), Some("spec.md"));
    assert_eq!(spec.sections.unwrap().len(), 5);
    assert!(catalog_stage("nope").is_none());
    assert_eq!(catalog_entry("spec").unwrap().phase, StagePhase::Design);
    assert!(catalog_entry("nope").is_none());
}

#[test]
fn holds_the_28_stages_of_the_spec_by_phase() {
    assert_eq!(
        ids_of(StagePhase::Understand),
        ["context", "research", "clarify"]
    );
    assert_eq!(
        ids_of(StagePhase::Design),
        ["spec", "ux-design", "architecture", "threat-model", "plan"]
    );
    assert_eq!(
        ids_of(StagePhase::Build),
        [
            "test-spec",
            "implement",
            "migration",
            "refactor",
            "docs",
            "i18n"
        ]
    );
    assert_eq!(
        ids_of(StagePhase::Check),
        [
            "lint",
            "self-review",
            "security",
            "performance",
            "accessibility",
            "agent-review",
            "review",
            "testing",
            "e2e",
            "qa",
            "design-review"
        ]
    );
    assert_eq!(
        ids_of(StagePhase::Ship),
        ["changelog", "staging", "release"]
    );
    let names: Vec<&str> = STAGE_PHASES.iter().map(|phase| phase.name()).collect();
    assert_eq!(names, ["Understand", "Design", "Build", "Check", "Ship"]);
}

#[test]
fn gives_each_stage_the_kind_gate_and_output_of_the_spec() {
    let shape = |id: &str| {
        let stage = catalog_stage(id).unwrap();
        (stage.kind, stage.gate, stage.output)
    };
    let md = |name: &str| Some(name.to_string());
    use Gate::{Auto, Manual};
    use StageKind::{Agent, Testing};
    assert_eq!(shape("research"), (Agent, Manual, md("research.md")));
    assert_eq!(shape("clarify"), (Agent, Manual, md("questions.md")));
    assert_eq!(shape("ux-design"), (Agent, Manual, md("ux.md")));
    assert_eq!(shape("migration"), (Agent, Manual, None));
    assert_eq!(shape("refactor"), (Agent, Auto, None));
    assert_eq!(shape("lint"), (Testing, Auto, None));
    assert_eq!(shape("self-review"), (Agent, Auto, md("self-review.md")));
    assert_eq!(shape("qa"), (StageKind::Manual, Manual, None));
    assert_eq!(shape("staging"), (StageKind::Manual, Manual, None));
    assert!(catalog_entry("lint").unwrap().needs_command);
    assert!(catalog_entry("e2e").unwrap().needs_command);
}

#[test]
fn gives_every_agent_and_testing_stage_a_prompt() {
    for stage in catalog() {
        if matches!(stage.kind, StageKind::Agent | StageKind::Testing) {
            assert!(
                default_prompt(stage).starts_with("Task: {title}\n"),
                "{}",
                stage.id
            );
        }
    }
}

#[test]
fn holds_valid_stages_with_unique_ids() {
    for entry in catalog_entries() {
        assert!(
            stage_problems(&entry.stage).is_empty(),
            "{}",
            entry.stage.id
        );
        assert_eq!(entry.stage.prompt, None);
    }
}

#[test]
fn has_a_prompt_for_each_agent_stage_and_a_check_for_each_output() {
    for CatalogStage { stage, prompt, .. } in catalog_entries() {
        if stage.kind == StageKind::Agent {
            assert!(prompt.is_some(), "{}", stage.id);
        }
        // The context stage has no prompt: its sources write the file.
        if let Some(output) = &stage.output
            && stage.kind != StageKind::Context
        {
            let text = prompt.unwrap_or_default();
            assert!(text.contains(&format!("{{dir}}/{output}")), "{}", stage.id);
        }
        if let Some(text) = prompt
            && stage.output.is_none()
            && stage.kind != StageKind::Context
        {
            // A stage that changes code must say whether to commit.
            assert!(text.contains("{commit}"), "{}", stage.id);
        }
    }
}

#[test]
fn has_every_stage_of_the_default_pipeline() {
    for step in default_pipeline() {
        for stage in step.stages() {
            assert_eq!(catalog_stage(&stage.id).as_ref(), Some(stage));
        }
    }
}

#[test]
fn only_lets_agent_and_manual_stages_run_side_by_side() {
    assert_eq!(GROUP_KINDS, [StageKind::Agent, StageKind::Manual]);
}
