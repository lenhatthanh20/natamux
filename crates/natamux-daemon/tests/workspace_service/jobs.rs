//! `describe('background jobs')`. The push is held while it runs: the listener of its
//! `jobs.changed` blocks (see `Gate`), so the job stays listed as long as the test needs.

use super::*;

#[test]
fn shows_a_push_as_a_job_while_it_runs_and_drops_it_after_also_on_failure() {
    let fx = Fx::new();
    // Made by the default service, so the gate holds the push and not the new workspace.
    let task = fx.with_workspace("Fix the bug");
    let (jobs, sent, gate) = Gate::jobs(&fx);
    let ws = fx.build(|options| options.jobs = Arc::clone(&jobs));

    // No origin, so the push fails.
    let (runner, id) = (Arc::clone(&ws), task.id.clone());
    let pushing = std::thread::spawn(move || runner.push(&id));
    gate.wait_started();
    let listed = jobs.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].label, "Pushing");
    assert_eq!(listed[0].task_id.as_deref(), Some(task.id.as_str()));
    // A second change is refused while the first runs, and makes no job.
    assert!(is_conflict(&ws.commit(&task.id, "x")));
    assert_eq!(jobs.list().len(), 1);
    gate.release();
    assert!(pushing.join().unwrap().is_err());

    assert!(jobs.list().is_empty());
    let counts: Vec<usize> = sent
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| match message {
            DaemonMessage::JobsChanged { jobs } => Some(jobs.len()),
            _ => None,
        })
        .collect();
    assert_eq!(counts, [1, 0]);
}
