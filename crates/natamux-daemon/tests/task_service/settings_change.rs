//! The group 'a task on its way when the settings change'.

use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, StageKind};

use super::*;

#[test]
fn keeps_its_stages_with_catalog_settings_for_a_stage_that_is_gone_and_live_prompts() {
    let fx = Fixture::new();
    let mut first = default_pipeline();
    let rest = first.split_off(2);
    let with_extra: Vec<Step> = first
        .into_iter()
        .chain([
            Step::Stage(Stage::new("research", StageKind::Agent, Gate::Auto)),
            Step::Stage(Stage::new("sign-off", StageKind::Agent, Gate::Auto)),
        ])
        .chain(rest)
        .collect();
    let (shared, settings_of) = live_settings(with_extra);
    let live = fx.make(Some(settings_of));
    let a = create(&live, "A");

    *shared.lock().unwrap() = default_pipeline()
        .into_iter()
        .map(|step| match step {
            Step::Stage(stage) if stage.id == "plan" => Step::Stage(Stage {
                prompt: Some("New plan prompt".into()),
                ..stage
            }),
            other => other,
        })
        .collect();
    let steps = live.steps_of(&live.get_task(&a.id).unwrap()).unwrap();

    let research = stage_of(&steps, "research");
    assert_eq!(research.kind, StageKind::Agent);
    assert_eq!(research.gate, Gate::Manual);
    assert_eq!(research.output.as_deref(), Some("research.md"));
    assert_eq!(
        *stage_of(&steps, "sign-off"),
        Stage::new("sign-off", StageKind::Manual, Gate::Manual)
    );
    assert_eq!(
        stage_of(&steps, "plan").prompt.as_deref(),
        Some("New plan prompt")
    );

    let b = create(&live, "B");
    assert!(!b.stages.contains(&json!("research")));
}
