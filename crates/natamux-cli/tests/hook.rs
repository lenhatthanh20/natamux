//! Ports of `packages/cli/src/__tests__/forward-hook.test.ts`, one test per case. The cases of
//! "the fast hook path" check how Node loads files and do not apply to Rust; the test
//! `the_hook_path_never_parses_the_command_line` replaces them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::{Duration, Instant};

use natamux_cli::run::{EXIT_OK, Env};
use serde_json::json;
use support::*;

const HOOK_JSON: &str = r#"{"hook_event_name":"Stop","cwd":"/work/T-0001"}"#;

fn ok_reply(_: &Call) -> Reply {
    json_reply(json!({}))
}

fn claude_env() -> Env {
    Env::from([("NATAMUX_TOKEN", "secret"), ("CLAUDECODE", "1")])
}

/// `natamux hook claude-code` with the stdin of the hook.
fn forward(env: Env, stdin: &[u8], handler: impl Fn(&Call) -> Reply + 'static) -> Outcome {
    run_with(&["hook", "claude-code"], env, stdin, handler)
}

fn hook(argv: &[&str], env: Env) -> Outcome {
    run_with(argv, env, HOOK_JSON.as_bytes(), ok_reply)
}

// forwardHook

#[test]
fn does_nothing_without_natamux_token_because_the_session_is_not_one_of_natamux() {
    let result = forward(
        Env::from([("CLAUDECODE", "1")]),
        HOOK_JSON.as_bytes(),
        ok_reply,
    );
    assert!(result.calls.is_empty());
    assert_eq!(result.stderr, "");
}

#[test]
fn drops_the_byte_order_mark_that_cursor_puts_first_on_windows() {
    let stdin = [b"\xEF\xBB\xBF".as_slice(), HOOK_JSON.as_bytes()].concat();
    let result = forward(claude_env(), &stdin, ok_reply);
    assert_eq!(result.calls[0].body_text(), HOOK_JSON);
}

#[test]
fn posts_the_json_of_the_hook_as_it_is_with_the_token() {
    let result = forward(claude_env(), HOOK_JSON.as_bytes(), ok_reply);
    assert_eq!(result.calls.len(), 1);
    let call = &result.calls[0];
    assert_eq!(call.url, "http://127.0.0.1:4317/hooks/claude-code");
    assert_eq!(call.method, "POST");
    assert_eq!(call.body_text(), HOOK_JSON);
    // The transport turns the token into the bearer header.
    assert_eq!(call.token, "secret");
    assert_eq!(call.header("content-type"), Some("application/json"));
}

#[test]
fn names_the_task_when_natamux_task_id_is_set_and_sends_no_header_when_it_is_not() {
    let named = forward(
        Env::from([
            ("NATAMUX_TOKEN", "secret"),
            ("CLAUDECODE", "1"),
            ("NATAMUX_TASK_ID", "T-0007"),
        ]),
        HOOK_JSON.as_bytes(),
        ok_reply,
    );
    assert_eq!(named.calls[0].header("x-natamux-task"), Some("T-0007"));

    let unnamed = forward(claude_env(), HOOK_JSON.as_bytes(), ok_reply);
    assert_eq!(unnamed.calls[0].header("x-natamux-task"), None);
}

#[test]
fn uses_natamux_url_and_drops_a_trailing_slash() {
    let result = forward(
        Env::from([
            ("NATAMUX_TOKEN", "secret"),
            ("CLAUDECODE", "1"),
            ("NATAMUX_URL", "http://example.test:9000/"),
        ]),
        HOOK_JSON.as_bytes(),
        ok_reply,
    );
    assert_eq!(
        result.calls[0].url,
        "http://example.test:9000/hooks/claude-code"
    );
}

#[test]
fn sends_nothing_when_stdin_is_empty() {
    let result = forward(claude_env(), b"  \n", ok_reply);
    assert!(result.calls.is_empty());
}

#[test]
fn sends_stdin_that_is_not_json_as_it_is_and_leaves_the_verdict_to_the_daemon() {
    let result = forward(claude_env(), b"{not json", ok_reply);
    assert_eq!(result.calls[0].body_text(), "{not json");
}

#[test]
fn sends_a_very_large_stdin_whole() {
    let big = format!(r#"{{"output":"{}"}}"#, "x".repeat(5 * 1024 * 1024));
    let result = forward(claude_env(), big.as_bytes(), ok_reply);
    assert_eq!(result.calls[0].body_text(), big);
}

#[test]
fn stays_silent_when_the_daemon_cannot_be_reached_or_answers_with_an_error() {
    let answers = forward(claude_env(), HOOK_JSON.as_bytes(), |_| raw_reply(500, "no"));
    assert_eq!(answers.code, EXIT_OK);
    assert_eq!(answers.stderr, "");

    let unreachable = forward(claude_env(), HOOK_JSON.as_bytes(), |_| Reply::Down);
    assert_eq!(unreachable.code, EXIT_OK);
    assert_eq!(unreachable.stderr, "");
}

#[test]
fn says_what_went_wrong_when_natamux_debug_is_set() {
    let debug_env = || {
        Env::from([
            ("NATAMUX_TOKEN", "secret"),
            ("CLAUDECODE", "1"),
            ("NATAMUX_DEBUG", "1"),
        ])
    };
    let answers = forward(debug_env(), HOOK_JSON.as_bytes(), |_| raw_reply(500, "no"));
    assert!(
        answers.stderr.contains("the daemon answered 500"),
        "{}",
        answers.stderr
    );

    let unreachable = forward(debug_env(), HOOK_JSON.as_bytes(), |_| Reply::Down);
    assert!(
        unreachable.stderr.contains("fetch failed"),
        "{}",
        unreachable.stderr
    );
}

// natamux hook

#[test]
fn forwards_the_hook_exits_0_and_prints_nothing() {
    let result = hook(&["hook", "claude-code"], claude_env());
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "");
    assert_eq!(result.stderr, "");
    assert_eq!(
        result.lines(),
        ["POST http://127.0.0.1:4317/hooks/claude-code"]
    );
}

#[test]
fn sends_nothing_when_another_agent_runs_claude_codes_hooks() {
    // Copilot CLI reads `.claude/settings.local.json` too, and does not set CLAUDECODE.
    let envs = [
        Env::from([("NATAMUX_TOKEN", "secret")]),
        Env::from([("NATAMUX_TOKEN", "secret"), ("CLAUDECODE", "")]),
        Env::from([("NATAMUX_TOKEN", "secret"), ("CLAUDECODE", "0")]),
    ];
    for env in envs {
        let result = hook(&["hook", "claude-code"], env);
        assert_eq!(result.code, EXIT_OK);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
        assert!(result.calls.is_empty());
    }
}

#[test]
fn names_the_event_of_a_copilot_hook_in_the_address() {
    let result = hook(
        &["hook", "copilot", "agentStop"],
        Env::from([("NATAMUX_TOKEN", "secret")]),
    );
    assert_eq!(
        result.lines(),
        ["POST http://127.0.0.1:4317/hooks/copilot?event=agentStop"]
    );
}

#[test]
fn answers_empty_json_for_the_agents_that_read_json_even_outside_a_natamux_session() {
    for agent in ["cursor", "gemini"] {
        assert_eq!(hook(&["hook", agent], Env::default()).stdout, "{}\n");
    }
    assert_eq!(hook(&["hook", "codex"], Env::default()).stdout, "");
}

#[test]
fn exits_0_and_prints_nothing_when_the_daemon_is_down_or_blocks_the_hook() {
    let replies: [fn(&Call) -> Reply; 2] = [|_| Reply::Down, |_| raw_reply(403, "blocked")];
    for reply in replies {
        let result = run_with(
            &["hook", "claude-code"],
            claude_env(),
            HOOK_JSON.as_bytes(),
            reply,
        );
        assert_eq!(result.code, EXIT_OK);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
    }
}

#[test]
fn exits_0_and_prints_nothing_outside_a_natamux_session() {
    let result = hook(&["hook", "claude-code"], Env::default());
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "");
    assert!(result.calls.is_empty());
}

#[test]
fn exits_0_even_for_a_wrong_command_so_the_agent_is_not_blocked() {
    let result = hook(&["hook"], Env::from([("NATAMUX_TOKEN", "secret")]));
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("agent name is required"));
    assert!(result.calls.is_empty());
}

#[test]
fn exits_0_and_names_the_problem_for_the_agent_name() {
    let cases: [&[&str]; 3] = [&["hook", "Claude Code"], &["hook", "a/b"], &["hook", "1x"]];
    for argv in cases {
        let result = hook(argv, Env::from([("NATAMUX_TOKEN", "secret")]));
        assert_eq!(result.code, EXIT_OK, "{argv:?}");
        assert!(result.stderr.contains("agent name is required"), "{argv:?}");
        assert!(result.calls.is_empty(), "{argv:?}");
    }
}

#[test]
fn takes_the_event_of_a_copilot_hook_and_nothing_more() {
    let cases: [&[&str]; 2] = [
        &["hook", "copilot", "agentStop", "extra"],
        &["hook", "copilot", "a/b"],
    ];
    for argv in cases {
        let result = hook(argv, Env::from([("NATAMUX_TOKEN", "secret")]));
        assert_eq!(result.code, EXIT_OK, "{argv:?}");
        assert!(result.stderr.contains("Unexpected argument"), "{argv:?}");
        assert!(result.calls.is_empty(), "{argv:?}");
    }
}

#[test]
fn names_only_the_extra_arguments_as_the_full_parser_did() {
    let result = hook(&["hook", "copilot", "agentStop", "a", "b"], Env::default());
    assert_eq!(
        result.stderr,
        "Unexpected argument: a b\nRun \"natamux help\" for usage.\n"
    );
}

// Replaces "the fast hook path" of the TypeScript tests.

#[test]
fn the_hook_path_never_parses_the_command_line() {
    // The command parser rejects each of these; the hook still exits 0 and prints nothing.
    let cases: [&[&str]; 4] = [
        &["hook", "claude-code", "--json"],
        &["hook", "claude-code", "--no-such-option"],
        &["hook", "claude-code", "--priority"],
        &["hook", "claude-code", "--help"],
    ];
    for argv in cases {
        let result = hook(argv, claude_env());
        assert_eq!(result.code, EXIT_OK, "{argv:?}");
        assert_eq!(result.stdout, "", "{argv:?}");
        assert!(result.calls.is_empty(), "{argv:?}");
    }
    // The same words after a plain command are a usage error, which shows the parser is not used.
    assert_eq!(
        run_with(
            &["task", "list", "--no-such-option"],
            claude_env(),
            b"",
            ok_reply
        )
        .code,
        natamux_cli::run::EXIT_USAGE
    );
}

#[test]
fn a_daemon_that_never_answers_ends_the_hook_within_the_timeout() {
    let started = Instant::now();
    let result = forward(claude_env(), HOOK_JSON.as_bytes(), |_| Reply::Never);
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "");
    assert_eq!(result.stderr, "");
    assert_eq!(result.calls.len(), 1);
    assert!(
        started.elapsed() < Duration::from_millis(3500),
        "{:?}",
        started.elapsed()
    );

    let debug_env = Env::from([
        ("NATAMUX_TOKEN", "secret"),
        ("CLAUDECODE", "1"),
        ("NATAMUX_DEBUG", "1"),
    ]);
    let started = Instant::now();
    let result = forward(debug_env, HOOK_JSON.as_bytes(), |_| Reply::Never);
    assert_eq!(result.code, EXIT_OK);
    assert!(result.stderr.contains("timeout"), "{}", result.stderr);
    assert!(
        started.elapsed() < Duration::from_millis(3500),
        "{:?}",
        started.elapsed()
    );
}
