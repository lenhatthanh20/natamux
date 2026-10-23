//! Scenarios 5, 6, 8 and 9: size, backpressure, resume, snapshot replay.

use std::time::Duration;

use natamux_contract::daemon::Daemon;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::{Transcript, dump_snapshot};
use serde_json::json;

use crate::{both, rows, run, shell};

/// What `stty size` printed last.
fn stty_size(socket: &Socket) -> String {
    let text = socket.text();
    text.lines()
        .rev()
        .find(|line| {
            let parts: Vec<&str> = line.trim().split(' ').collect();
            parts.len() == 2 && parts.iter().all(|part| part.parse::<u16>().is_ok())
        })
        .unwrap_or("")
        .trim()
        .to_string()
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn resize_and_who_claims_the_size() {
    both("resize", &[], |d: Daemon, mut t: Transcript| async move {
        let mut first = shell(&d, 5, "cols=80&rows=24").await;
        run(&mut first, "stty size", 1).await;
        t.note("first size", json!(stty_size(&first)));
        first
            .send(r#"{"type":"resize","cols":100,"rows":30}"#)
            .await;
        run(&mut first, "stty size", 2).await;
        t.note("after resize", json!(stty_size(&first)));
        let mut hidden = shell(&d, 5, "cols=120&rows=40&visible=0").await;
        run(&mut first, "stty size", 3).await;
        t.note("a hidden client takes no size", json!(stty_size(&first)));
        hidden
            .send(r#"{"type":"visible","visible":true,"cols":90,"rows":20}"#)
            .await;
        run(&mut hidden, "stty size", 4).await;
        t.note(
            "a client that comes on screen takes it",
            json!(stty_size(&hidden)),
        );
        t
    })
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn backpressure_and_catch_up() {
    both(
        "backpressure",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            let mut slow = shell(&d, 6, "cols=80&rows=24").await;
            let mut hidden = shell(&d, 6, "cols=80&rows=24&visible=0").await;
            let start = slow.bytes().len();
            slow.input("head -c 3000000 /dev/zero | tr '\\0' x; echo; echo FLOOD-$((1+1))\r")
                .await;
            // The slow client never acks: it gets at most CLIENT_HIGH_WATER of live output.
            slow.read_for(Duration::from_secs(3)).await;
            hidden.read_for(Duration::from_millis(10)).await;
            let live = (slow.bytes().len() - start) as u64;
            t.note(
                "stops at the high water mark",
                json!((64 * 1024..=512 * 1024).contains(&live)),
            );
            let hidden_live = hidden.bytes().len() as u64;
            t.note(
                "a hidden client gets a budget",
                json!(hidden_live <= 4 * 256 * 1024),
            );
            // Acks of all it got: it is caught up, by the ring or a snapshot, to the end.
            let replayed = slow.controls()[2]["offset"].as_u64().unwrap_or(0);
            let ack = json!({ "type": "ack", "offset": replayed + live }).to_string();
            slow.send(&ack).await;
            let caught = slow
                .until(|socket| socket.text().contains("FLOOD-2") && socket.count("replayed") >= 2)
                .await;
            t.note("caught up", json!(caught));
            run(&mut slow, "true", 1).await;
            t.note("screen", rows(&slow, 80, 24));
            t
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn reconnect_with_resume() {
    both("resume", &[], |d: Daemon, mut t: Transcript| async move {
        let mut first = shell(&d, 8, "cols=80&rows=24").await;
        run(&mut first, "echo one", 1).await;
        let controls = first.controls();
        let shell_id = controls[0]["shellId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let replayed = controls[2]["offset"].as_u64().unwrap_or(0);
        let offset = replayed + first.bytes().len() as u64
            - first.last_snapshot().map_or(0, |s| s.2.len() as u64);
        first.drop_connection();
        let mut other = shell(&d, 8, "cols=80&rows=24").await;
        run(&mut other, "echo missed", 2).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let query =
            format!("terminal=8&cols=80&rows=24&visible=0&shellId={shell_id}&offset={offset}");
        let mut back = Socket::open(&d, &query).await;
        back.until(|socket| socket.count("replayed") > 0).await;
        t.note("resume", json!(back.controls()));
        t.note(
            "missed output",
            json!(back.text().contains("missed") && back.text().contains("END-2")),
        );

        let stale = format!("terminal=8&cols=80&rows=24&visible=0&shellId={shell_id}&offset=1");
        let mut old = Socket::open(&d, &stale).await;
        old.until(|socket| socket.count("replayed") > 0).await;
        t.note("an offset the ring does not have", json!(old.types()));
        t
    })
    .await;
}

const STYLES: &str = r#"printf '\e[1;31mbold red\e[0m \e[38;5;33mindex\e[0m \e[48;2;10;20;30mrgb\e[0m \e[3;4;9mstyle\e[0m \e[7mreverse\e[0m\n中文字 Tiếng Việt có dấu 😀\n'; printf 'w%.0s' $(seq 1 200); echo"#;

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn snapshot_replay() {
    both("snapshot", &[], |d: Daemon, mut t: Transcript| async move {
        let mut writer = shell(&d, 9, "cols=80&rows=24").await;
        run(&mut writer, "seq 1 10000", 1).await;
        run(&mut writer, STYLES, 2).await;
        run(&mut writer, r"printf '\e[?1002h\e[?1006h\e[?25l'", 3).await;
        let reader = shell(&d, 9, "cols=80&rows=24&visible=0").await;
        let snapshot = reader.last_snapshot().unwrap_or_default();
        dump_snapshot("history", d.name, &snapshot);
        let all = reader.screen(80, 24);
        t.note("history rows", json!(all.len()));
        t.note(
            "history top and end",
            json!([all.first(), all.get(all.len().saturating_sub(30)..)]),
        );

        run(&mut writer, r"printf '\e[?1049h\e[HALT SCREEN\n'", 4).await;
        let mut alt = shell(&d, 9, "cols=80&rows=24&visible=0").await;
        let snapshot = alt.last_snapshot().unwrap_or_default();
        dump_snapshot("alternate", d.name, &snapshot);
        t.note("alternate screen", rows(&alt, 80, 24));
        alt.input("printf '\\e[?1049l'\r").await;
        alt.read_for(Duration::from_millis(500)).await;
        t
    })
    .await;
}
