//! The group 'stopAgents'.

use natamux_proto::tasks::task::TaskState;

use super::*;

fn running_task(fx: &Fixture) -> Task {
    let task = create(&fx.service, "First");
    human(&fx.service, &task.id, "task.readied");
    apply(
        &fx.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.started" }),
    );
    task
}

#[test]
fn makes_a_running_stage_wait_to_start_again() {
    let fx = Fixture::new();
    let task = running_task(&fx);

    assert_eq!(
        fx.service.stop_agents(&task.id).unwrap().state,
        TaskState::Ready
    );
    let event = last_event(&fx.service, &task.id).event;
    assert_eq!(event.kind, "agent.stopped");
    assert_eq!(event.payload["stage"], "implement");
}

#[test]
fn makes_a_stage_whose_agent_waited_for_you_wait_to_start_again() {
    let fx = Fixture::new();
    let task = running_task(&fx);
    apply(
        &fx.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.asked" }),
    );

    assert_eq!(
        fx.service.stop_agents(&task.id).unwrap().state,
        TaskState::Ready
    );
}

#[test]
fn records_nothing_for_a_task_with_no_agent_at_work() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    assert_eq!(fx.service.stop_agents(&task.id).unwrap(), task);
    assert_eq!(fx.service.list_events(&task.id).unwrap().len(), 1);
}
