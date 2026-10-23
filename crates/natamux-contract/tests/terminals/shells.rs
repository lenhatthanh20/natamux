//! Scenarios 7, 10 and 11: close codes and the limit, restart and restore, the end of a shell.

use hyper::Method;
use natamux_contract::daemon::Daemon;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::Transcript;
use serde_json::json;

use crate::{both, run, shell};

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn close_codes_and_the_limit() {
    both(
        "close codes",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            let mut first = shell(&d, 7, "cols=80&rows=24").await;
            let shell_id = first.controls()[0]["shellId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            first.input("exit\r").await;
            t.note("exit", json!(first.close_code().await));
            let gone = format!("terminal=7&cols=80&rows=24&shellId={shell_id}&offset=0");
            let mut resumed = Socket::open(&d, &gone).await;
            t.note(
                "resume of an ended shell",
                json!([resumed.close_code().await, resumed.frames.len()]),
            );

            let mut open = Vec::new();
            for terminal in 100..130 {
                open.push(shell(&d, terminal, "cols=80&rows=24").await);
            }
            let mut refused = Socket::open(&d, "terminal=130&cols=80&rows=24").await;
            let code = refused.close_code().await;
            t.note("the 31st terminal", json!([code, refused.controls()]));
            t
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn restart_and_restore() {
    both("restart", &[], |d: Daemon, mut t: Transcript| async move {
        let path = format!("/tasks/{}/terminals", d.task);
        // Shells closed by the scenarios before stay in the open-shells list for EXIT_CONFIRM_MS.
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        let mut first = shell(&d, 10, "cols=80&rows=24").await;
        run(&mut first, "echo before", 1).await;
        t.note("hello", json!(first.controls()[0]));
        first.drop_connection();
        d.restart().await;
        t.note(
            "after the restart",
            json!(d.call(Method::GET, &path, true).await),
        );
        let mut back = shell(&d, 10, "cols=80&rows=24").await;
        t.note("a new shell", json!(back.controls()[0]));
        run(&mut back, "echo after", 2).await;
        t.note("listed", json!(d.call(Method::GET, &path, true).await));
        t
    })
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn the_end_of_a_shell() {
    both("end", &[], |d: Daemon, mut t: Transcript| async move {
        let mut ending = shell(&d, 11, "cols=80&rows=24").await;
        ending.input("exit 3\r").await;
        let code = ending.close_code().await;
        t.note("exit 3", json!([code, ending.controls().last()]));
        let mut closed = shell(&d, 12, "cols=80&rows=24").await;
        let path = format!("/tasks/{}/terminals/12", d.task);
        t.note("delete", json!(d.call(Method::DELETE, &path, true).await));
        let code = closed.close_code().await;
        t.note("closed by delete", json!([code, closed.controls().last()]));
        let ended = format!("/tasks/{}/ended-shells", d.task);
        t.note(
            "ended shells",
            json!(d.call(Method::GET, &ended, true).await),
        );
        t
    })
    .await;
}
