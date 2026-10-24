//! 49 `agent_makes_workspace`, 50 `delete_with_workspace` and 51 `dirty_after_hooks_and_output`, in
//! the `workspaces` module (the CI filter `workspaces::`).

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon};
use natamux_contract::events::Events;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use super::{
    REPO_DIR, alias, branch_of, dirty_count, make_task, open_workspace, retire, wait_exec,
    workspace_of,
};
use crate::hooks::Agent;
use crate::{both, call, get, kept, on_task, post, stage_events, twin};

/// Scenario 49: `POST /tasks/:id/agent` on a task without a workspace makes the workspace and
/// launches the agent; an unknown agent is refused first and makes no workspace.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn agent_makes_workspace() {
    both(
        "agent_makes_workspace",
        4901,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let unknown = twin(&d, 4902);
            alias(&mut t, &unknown, "<task 2>");
            make_task(&d, 4901, "Agent makes it").await;
            let body = json!({ "agent": "claude-code" });
            t.note(
                "agent",
                kept(&post(&d, &format!("/tasks/{id}/agent"), body).await),
            );
            workspace_of(&d, &id).await;
            t.note("task", kept(&get(&d, &format!("/tasks/{id}")).await));
            t.note("events", stage_events(&d, &id).await);
            make_task(&d, 4902, "Unknown agent").await;
            let body = json!({ "agent": "nope" });
            let path = format!("/tasks/{unknown}/agent");
            t.note("unknown agent", kept(&post(&d, &path, body).await));
            t.note(
                "no workspace",
                kept(&get(&d, &format!("/tasks/{unknown}")).await),
            );
            retire(&d, &id).await;
            retire(&d, &unknown).await;
            t
        },
    )
    .await;
}

/// Scenario 50: deleting a task removes its worktree and branch; a change that is not committed
/// refuses it (409) and keeps both; `DELETE /tasks/:id/workspace?deleteBranch=true` says
/// `branchDeleted`; a bad `deleteBranch` is refused; the stage files are archived.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn delete_with_workspace() {
    both(
        "delete_with_workspace",
        5001,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let other = twin(&d, 5002);
            alias(&mut t, &other, "<task 2>");
            make_task(&d, 5001, "Delete").await;
            let ws = open_workspace(&d, &id).await;
            let branch = branch_of(&d, &id).await;
            let exists = |ws: &str| format!("test -d '{ws}' && echo yes || echo no");
            d.exec_in(&format!("cd '{ws}' && printf 'dirty\\n' > dirty.txt")).await;
            let refused = call(&d, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
            t.note("delete with a change", kept(&refused));
            t.note("worktree kept", json!(d.exec_in(&exists(&ws)).await.trim()));
            t.note("task kept", kept(&get(&d, &format!("/tasks/{id}")).await));
            let listed = d.git_in(REPO_DIR, &format!("branch --list '{branch}'")).await;
            t.note("branch kept", json!(listed.trim()));
            d.git_in(&ws, "clean -fdq").await;
            let gone = call(&d, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
            // The answer lists every task of the daemon, which differs; the id is what counts.
            t.note("delete", json!([gone.status, gone.body["id"]]));
            t.note("worktree gone", json!(d.exec_in(&exists(&ws)).await.trim()));
            let listed = d.git_in(REPO_DIR, &format!("branch --list '{branch}'")).await;
            t.note("branch gone", json!(listed.trim()));
            t.note("task gone", kept(&get(&d, &format!("/tasks/{id}")).await));

            make_task(&d, 5002, "Remove the workspace").await;
            let ws = open_workspace(&d, &other).await;
            let stage_file = format!("cd '{ws}' && mkdir -p .natamux-task && printf '# Notes\\n' > .natamux-task/notes.md");
            d.exec_in(&stage_file).await;
            // The stage folder is ignored once the workspace is set up, so a clean keeps it.
            let ignored = format!(
                "cd '{ws}' && test \"$(git status --porcelain | grep -c natamux-task)\" = 0 && echo yes"
            );
            wait_exec(&d, &ignored, "the stage folder ignored").await;
            d.git_in(&ws, "clean -fdq").await;
            let path = format!("/tasks/{other}/workspace");
            let bad = call(&d, Method::DELETE, &format!("{path}?deleteBranch=maybe"), Body::None).await;
            t.note("bad deleteBranch", kept(&bad));
            let removed = call(&d, Method::DELETE, &format!("{path}?deleteBranch=true"), Body::None).await;
            t.note("remove workspace", kept(&removed));
            let archive = format!("ls '{}/tasks/{other}'", d.data_dir());
            t.note("archived", json!(d.exec_in(&archive).await.trim()));
            t.note("remove again", kept(&call(&d, Method::DELETE, &path, Body::None).await));
            retire(&d, &other).await;
            t
        },
    )
    .await;
}

/// Scenario 51: a `PostToolUse` hook makes `workspace.dirty`, and so does output in Terminal 1
/// once it goes quiet. Positive waits only: that `PreToolUse` touches nothing is a unit test (a
/// negative needs a timer). Earlier messages of the same task could end a wait, so each wait is
/// for one more than what was seen.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn dirty_after_hooks_and_output() {
    both(
        "dirty_after_hooks_and_output",
        5101,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            make_task(&d, 5101, "Dirty").await;
            open_workspace(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            let seen = dirty_count(&events.messages, &id);
            t.note("tool ran", agent.claude("PostToolUse", json!({})).await);
            let more = |m: &[Value]| dirty_count(m, &id) > seen;
            t.note("dirty after the hook", json!(events.until(more).await));
            let seen = dirty_count(&events.messages, &id);
            let mut socket =
                Socket::open(&on_task(&d, &id), "terminal=1&cols=120&rows=30&visible=1").await;
            socket.until(|socket| socket.count("replayed") > 0).await;
            socket.input("echo contract-output\r").await;
            socket
                .until(|socket| socket.text().contains("contract-output"))
                .await;
            let more = |m: &[Value]| dirty_count(m, &id) > seen;
            t.note("dirty after output", json!(events.until(more).await));
            drop(socket);
            retire(&d, &id).await;
            t
        },
    )
    .await;
}
