//! `natamux security`: the security mode and its keys (T-0373 Part 6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_cli::run::{EXIT_OK, EXIT_USAGE};
use serde_json::{Value, json};
use support::*;

fn strict_body() -> Value {
    json!({
        "strict": true,
        "keys": {
            "agentApi.grants": { "value": "", "blocked": false, "reason": null },
            "ciPush": { "value": "forbid", "blocked": true, "reason": "No CI push." },
        },
        "ignored": ["agents.permissionMode"],
        "warning": "security.json changed outside Natamux.",
    })
}

#[test]
fn json_prints_the_answer_of_the_security_route() {
    let result = run(&["security", "--json"], |_| json_reply(strict_body()));
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert_eq!(result.lines(), ["GET http://127.0.0.1:4317/security"]);
    let printed: Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(printed, strict_body());
}

#[test]
fn text_names_the_mode_the_warning_and_each_key() {
    let result = run(&["security"], |_| json_reply(strict_body()));
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        concat!(
            "Security mode: Strict\n",
            "Warning: security.json changed outside Natamux.\n",
            "  agentApi.grants: \n",
            "  ciPush: forbid (blocked: No CI push.)\n",
            "Ignored (looser than Strict): agents.permissionMode\n",
        )
    );
    let personal = json!({ "strict": false, "keys": {}, "ignored": [], "warning": null });
    let result = run(&["security"], move |_| json_reply(personal.clone()));
    assert_eq!(result.stdout, "Security mode: Personal\n");
}

#[test]
fn text_tells_the_isolation_of_the_daemon_run() {
    let ready = json!({ "strict": false, "isolation":
        { "mode": "required", "backend": "landlock", "abi": 5, "ok": true, "reason": null } });
    let result = run(&["security"], move |_| json_reply(ready.clone()));
    assert_eq!(
        result.stdout,
        "Security mode: Personal\nIsolation: required (landlock, ABI 5)\n"
    );
    let broken = json!({ "strict": false, "isolation":
        { "mode": "required", "backend": "none", "abi": null, "ok": false, "reason": "No Landlock" } });
    let result = run(&["security"], move |_| json_reply(broken.clone()));
    assert!(
        result
            .stdout
            .ends_with("Isolation: required, cannot run: No Landlock\n")
    );
    let off = json!({ "strict": false, "isolation":
        { "mode": "off", "backend": "none", "abi": null, "ok": false, "reason": "No Landlock" } });
    let result = run(&["security"], move |_| json_reply(off.clone()));
    assert!(result.stdout.ends_with("Isolation: off\n"));
}

#[test]
fn takes_no_argument() {
    let result = run(&["security", "off"], |_| json_reply(json!({})));
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.calls.is_empty());
}
