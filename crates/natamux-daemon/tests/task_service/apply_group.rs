//! The group 'applyEvent inside a group'.

use natamux_proto::stages::catalog::default_pipeline;

use super::*;

fn staged(fx: &Fixture) -> TaskService {
    fx.make(Some(settings(default_pipeline())))
}

fn at_build(service: &TaskService) -> String {
    let task = create(service, "Export");
    override_to(service, &task.id, "running", Some("build"));
    task.id
}

fn lanes(task: &Task) -> Map<String, Value> {
    task.lanes.clone()
}

#[test]
fn takes_an_event_with_no_stage_for_the_only_stage_it_fits_and_logs_that_stage() {
    let fx = Fixture::new();
    let staged = staged(&fx);
    let id = at_build(&staged);
    override_to(&staged, &id, "review", Some("test-spec"));

    let result = human(&staged, &id, "review.approved");

    assert!(result.accepted);
    assert_eq!(
        lanes(&result.task),
        obj(json!({ "test-spec": "done", "implement": "running" }))
    );
    assert_eq!(
        last_payload(&staged, &id),
        obj(json!({ "stage": "test-spec" }))
    );
}

#[test]
fn refuses_an_event_that_fits_two_stages_unless_its_terminal_names_one() {
    let fx = Fixture::new();
    let staged = staged(&fx);
    let id = at_build(&staged);

    let unclear = apply(
        &staged,
        &id,
        json!({ "source": "agent", "type": "agent.asked" }),
    );
    assert!(!unclear.accepted);

    let from_terminal = apply(
        &staged,
        &id,
        json!({
            "source": "agent", "type": "agent.asked",
            "payload": { "question": "Which file?", "terminal": "2" }
        }),
    );
    assert_eq!(
        lanes(&from_terminal.task),
        obj(json!({ "test-spec": "needs-input", "implement": "running" }))
    );
    assert_eq!(last_payload(&staged, &id)["stage"], "test-spec");
}

// The settings then made Test spec the stage that changes code, so they would give it Shell 1.
#[test]
fn takes_the_stage_pinned_to_the_terminal_of_a_bell_after_a_settings_change() {
    let fx = Fixture::new();
    let (shared, settings_of) = live_settings(default_pipeline());
    let service = fx.make(Some(settings_of));
    let id = create(&service, "Export").id;
    override_to(&service, &id, "ready", Some("build"));
    for (stage, terminal) in [("implement", "1"), ("test-spec", "2")] {
        apply(
            &service,
            &id,
            json!({ "source": "system", "type": "stage.started", "payload": { "stage": stage, "terminal": terminal } }),
        );
    }
    assert_eq!(
        lanes(&service.get_task(&id).unwrap()),
        obj(json!({ "test-spec": "running", "implement": "running" }))
    );
    *shared.lock().unwrap() = default_pipeline()
        .into_iter()
        .map(|step| match step {
            Step::Group(mut group) => {
                for stage in &mut group.parallel {
                    if stage.id == "implement" {
                        stage.output = Some("impl.md".into());
                    }
                }
                Step::Group(group)
            }
            other => other,
        })
        .collect();

    let bell = apply(
        &service,
        &id,
        json!({
            "source": "agent", "type": "agent.asked",
            "payload": { "reason": "terminal bell", "terminal": "1" }
        }),
    );

    assert_eq!(
        lanes(&bell.task),
        obj(json!({ "test-spec": "running", "implement": "needs-input" }))
    );
    assert_eq!(last_payload(&service, &id)["stage"], "implement");
}

#[test]
fn stops_every_stage_whose_agent_runs_or_waits_and_leaves_the_others() {
    let fx = Fixture::new();
    let staged = staged(&fx);
    let id = at_build(&staged);
    override_to(&staged, &id, "review", Some("test-spec"));
    apply(
        &staged,
        &id,
        json!({ "source": "agent", "type": "agent.asked", "payload": { "stage": "implement" } }),
    );

    let task = staged.stop_agents(&id).unwrap();

    assert_eq!(
        lanes(&task),
        obj(json!({ "test-spec": "review", "implement": "ready" }))
    );
    let event = last_event(&staged, &id).event;
    assert_eq!(
        (event.kind.as_str(), event.source.as_str()),
        ("agent.stopped", "system")
    );
    assert_eq!(event.payload["stage"], "implement");
}
