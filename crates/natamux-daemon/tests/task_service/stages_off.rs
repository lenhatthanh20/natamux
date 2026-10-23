//! The group 'createTask with stages that are off'.

use natamux_proto::stages::catalog::default_pipeline;

use super::*;

#[test]
fn gives_a_new_task_only_the_stages_that_are_on() {
    let fx = Fixture::new();
    let stages: Vec<Step> = default_pipeline()
        .into_iter()
        .map(|step| match step {
            Step::Stage(stage) if stage.id == "context" || stage.id == "plan" => {
                Step::Stage(Stage {
                    enabled: Some(false),
                    ..stage
                })
            }
            other => other,
        })
        .collect();
    let off = fx.make(Some(settings(stages)));

    let task = create(&off, "Export");

    assert_eq!(
        task.stages,
        [
            json!("spec"),
            json!({ "id": "build", "parallel": ["test-spec", "implement"] }),
            json!("testing"),
            json!("agent-review"),
            json!("release"),
        ]
    );
    assert_eq!(task.stage, "spec");
}
