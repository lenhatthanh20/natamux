//! 24 `gates_and_checks`, 25 `branch`, 26 `group`, 27 `send_back`.

use natamux_contract::daemon::Daemon;
use natamux_contract::transcript::{Allowed, Transcript};
use serde_json::json;

use crate::{
    act, both, cancel, event, launch_script, make_twins, report, stage_events, start,
    started_terminal, task, terminals, wait_at, wait_event, wait_file,
};

/// A missing section fails the check, not finally: the reasons go to the agent, which fixes
/// them. The manual gate waits; after the approve, the branch takes the `scoped` path (the spec
/// has a Scope), the plan passes its auto gate, and both lanes of the group start.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn gates_and_checks() {
    make_twins(&[2401], "[miss-section] Gates", "").await;
    // The fake agent ends its turn after it wrote the spec; on Node that hook sets the stage to
    // needs-input, sooner or later than the report. Rust hears no hook before part 4.
    let hooks = "state: Node's hook of the agent's turn end (needs-input); hooks are part 4";
    let allowed = [
        Allowed {
            step: "first report",
            reason: hooks,
        },
        Allowed {
            step: "after the failed check",
            reason: hooks,
        },
    ];
    both(
        "gates_and_checks",
        2401,
        &allowed,
        |d: Daemon, id, mut t: Transcript| async move {
            start(&d, &id).await;
            wait_file(&d, &id, "spec.md", "## Goal").await;
            t.note("first report", report(&d, &id, "spec", "1").await);
            t.note("after the failed check", task(&d, &id).await);
            wait_file(&d, &id, "spec.md", "## Scope").await;
            t.note("second report", report(&d, &id, "spec", "1").await);
            wait_at(&d, &id, "spec", "review").await;
            t.note(
                "approve",
                act(&d, &id, json!({ "action": "approve" })).await,
            );
            wait_event(&d, &id, "stage.started", "plan", 1).await;
            t.note("on the scoped path", task(&d, &id).await);
            // The fake agent is a bash script: 30 s after its launch nothing tells it from a shell,
            // so the plan's prompt would be typed as a new launch. The person passes the plan.
            let pass = json!({ "action": "pass", "stage": "plan" });
            t.note("plan passed", act(&d, &id, pass).await);
            wait_event(&d, &id, "stage.started", "implement", 1).await;
            wait_event(&d, &id, "stage.started", "notes", 1).await;
            t.note("at the group", task(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// A command condition holds (`small.flag` in the worktree): the path `small`. The approve comes
/// as an event of a person, which part 2 refused with 409.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn branch() {
    make_twins(&[2501], "Branch", "touch small.flag").await;
    both(
        "branch",
        2501,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            start(&d, &id).await;
            wait_file(&d, &id, "spec.md", "## Scope").await;
            report(&d, &id, "spec", "1").await;
            wait_at(&d, &id, "spec", "review").await;
            let approved = event(&d, &id, "stage.approved", json!({ "stage": "spec" })).await;
            t.note("approved by an event", approved);
            wait_event(&d, &id, "stage.started", "patch", 1).await;
            t.note("on the small path", task(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// Two lanes in Terminal 1 and the shell of the stage with its own agent; each terminal shows
/// its lane. A lane that passes ends its shell (`endShell`); the step ends when both did, and the
/// test command starts in a shell of its own.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn group() {
    make_twins(&[2601], "[no-done] Group", "").await;
    both(
        "group",
        2601,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let to_build = json!({ "to": "ready", "stage": "build" });
            t.note(
                "to the group",
                event(&d, &id, "state.overridden", to_build).await,
            );
            wait_event(&d, &id, "stage.started", "implement", 1).await;
            wait_event(&d, &id, "stage.started", "notes", 1).await;
            t.note("both lanes", task(&d, &id).await);
            let shell = started_terminal(&d, &id, "notes").await;
            t.note("notes shell", json!(shell));
            t.note("notes launch", launch_script(&d, &id, &shell).await);
            t.note("terminals of the lanes", terminals(&d, &id).await);
            let passed = act(&d, &id, json!({ "action": "pass", "stage": "notes" })).await;
            t.note("notes passed", passed);
            t.note("terminals after notes", terminals(&d, &id).await);
            let passed = act(&d, &id, json!({ "action": "pass", "stage": "implement" })).await;
            t.note("implement passed", passed);
            wait_event(&d, &id, "stage.started", "tests", 1).await;
            t.note("at the tests", task(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// A person sends the task back to Implement with a reason: the agent gets the stage prompt with
/// "What to change". A send back to a later or an unknown stage is refused.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn send_back() {
    make_twins(&[2701], "[no-done] Send back", "").await;
    both(
        "send_back",
        2701,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let to_review = json!({ "to": "review", "stage": "review" });
            t.note(
                "at the review",
                event(&d, &id, "state.overridden", to_review).await,
            );
            for (step, to) in [
                ("to a later stage", "release"),
                ("to an unknown stage", "nope"),
            ] {
                let body = json!({ "action": "reopen", "to": to });
                t.note(step, act(&d, &id, body).await);
            }
            let body = json!({ "action": "reopen", "to": "implement", "reason": "Fix the export" });
            t.note("back to implement", act(&d, &id, body).await);
            wait_event(&d, &id, "stage.started", "implement", 1).await;
            t.note("launch", launch_script(&d, &id, "1").await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}
