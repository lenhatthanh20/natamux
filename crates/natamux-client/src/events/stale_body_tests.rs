use natamux_proto::agents::activity::AgentActivity;

use super::*;

#[test]
fn activity_usage_and_jobs_come_from_their_bodies_and_limits_are_still_read() {
    let mut stale = Stale::default();
    let none = BTreeSet::new();
    stale.note(
        r#"{"kind":"activity.changed","taskId":"T-1","activity":{"1":{"text":"Edit a.ts","at":"t"}}}"#,
        &none,
    );
    stale.note(
        r#"{"kind":"activity.changed","taskId":"T-2","activity":{}}"#,
        &none,
    );
    stale.note(
        r#"{"kind":"usage.changed","taskId":"T-1","usage":{"tokens":3,"costUsd":0.25,"unpricedModels":[],"stages":[]}}"#,
        &none,
    );
    stale.note(
        r#"{"kind":"jobs.changed","jobs":[{"id":"j1","kind":"fetch","label":"Fetch","startedAt":"t"}]}"#,
        &none,
    );
    stale.note(r#"{"kind":"limits.changed","limits":[]}"#, &none);
    assert!(!stale.activity && !stale.usage && !stale.jobs);
    // The daemon's GET leaves out windows that reset; the message body does not.
    assert!(stale.limits);
    let line = AgentActivity {
        text: "Edit a.ts".into(),
        at: "t".into(),
    };
    let usage = TaskUsage {
        tokens: 3,
        cost_usd: Some(0.25),
        ..TaskUsage::default()
    };
    let job = Job {
        id: "j1".into(),
        kind: "fetch".into(),
        label: "Fetch".into(),
        task_id: None,
        started_at: "t".into(),
    };
    assert_eq!(
        stale.passed,
        [
            DashboardEvent::ActivityOf {
                task_id: "T-1".into(),
                activity: [("1".to_string(), line)].into(),
            },
            DashboardEvent::ActivityOf {
                task_id: "T-2".into(),
                activity: TaskActivity::new(),
            },
            DashboardEvent::UsageOf {
                task_id: "T-1".into(),
                usage,
            },
            DashboardEvent::Jobs(vec![job]),
        ]
    );
}

#[test]
fn a_body_the_feed_cannot_read_marks_its_part_for_a_read() {
    let mut stale = Stale::default();
    let none = BTreeSet::new();
    stale.note(r#"{"kind":"activity.changed","taskId":"T-1"}"#, &none);
    stale.note(r#"{"kind":"usage.changed","usage":{}}"#, &none);
    stale.note(r#"{"kind":"jobs.changed","jobs":[{"id":1}]}"#, &none);
    assert!(stale.activity && stale.usage && stale.jobs);
    assert!(stale.passed.is_empty());
}
