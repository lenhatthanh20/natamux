use super::*;
use serde_json::json;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("usage")
        .join(name)
}

#[test]
fn codex_limits_come_from_the_last_token_count_that_has_them() {
    let windows = read_codex_limits(&fixture("codex-rollout-limits.jsonl"));
    assert_eq!(
        windows,
        Some(vec![LimitWindow {
            minutes: 300,
            used_percent: 42.5,
            resets_at: Some("2026-10-27T08:00:00.000Z".into()),
        }])
    );
}

#[test]
fn a_codex_file_without_limits_has_none() {
    assert_eq!(read_codex_limits(&fixture("codex-rollout.jsonl")), None);
    assert_eq!(read_codex_limits(&fixture("no-such-file.jsonl")), None);
}

#[test]
fn claude_limits_read_both_windows_and_skip_a_missing_one() {
    let input = json!({
        "five_hour": { "used_percentage": 84, "resets_at": 1793088000 },
        "seven_day": { "used_percentage": 41.2 }
    });
    assert_eq!(
        claude_limits(&input),
        vec![
            LimitWindow {
                minutes: 300,
                used_percent: 84.0,
                resets_at: Some("2026-10-27T08:00:00.000Z".into()),
            },
            LimitWindow {
                minutes: 10_080,
                used_percent: 41.2,
                resets_at: None,
            },
        ]
    );
    assert_eq!(claude_limits(&json!({ "seven_day": {} })), Vec::new());
}
