//! The plan limits the usage service keeps.

use natamux_daemon::agents::usage_service::{current_limits, merge_windows};
use natamux_proto::agents::limits::{AgentLimits, LimitWindow};

use super::*;

fn window(minutes: u32, used_percent: f64, resets_at: Option<&str>) -> LimitWindow {
    LimitWindow {
        minutes,
        used_percent,
        resets_at: resets_at.map(str::to_string),
    }
}

const LATER: &str = "2999-01-01T00:00:00.000Z";

#[test]
fn keeps_the_limits_an_agent_told_and_sends_them_once() {
    let fx = Fx::new();
    let windows = vec![window(300, 84.0, Some(LATER)), window(10_080, 41.0, None)];

    fx.service.set_limits("claude-code", windows.clone());
    fx.service.set_limits("claude-code", windows.clone());

    let limits = fx.service.limits();
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0].agent, "claude-code");
    assert_eq!(limits[0].windows, windows);
    let sent: Vec<_> = fx
        .messages()
        .into_iter()
        .filter(|message| matches!(message, DaemonMessage::LimitsChanged { .. }))
        .collect();
    assert_eq!(sent.len(), 1);
    // A restart reads them back.
    assert_eq!(fx.restarted(false).limits(), limits);
}

#[test]
fn no_windows_change_nothing() {
    let fx = Fx::new();
    fx.service.set_limits("claude-code", Vec::new());
    assert!(fx.service.limits().is_empty());
    assert!(fx.messages().is_empty());
}

#[test]
fn reads_codex_limits_from_its_session_file() {
    let fx = Fx::new();
    let session = fx.home.join(".codex").join("rollout.jsonl");
    std::fs::create_dir_all(session.parent().unwrap()).unwrap();
    let line = json!({
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": { "total_token_usage": { "input_tokens": 10, "output_tokens": 1 } },
            "rate_limits": {
                "primary": { "used_percent": 12.5, "window_minutes": 300, "resets_at": 32503680000_u64 }
            }
        }
    });
    std::fs::write(&session, format!("{line}\n")).unwrap();

    fx.read(Some("implement"), "codex", &session);

    let limits = fx.service.limits();
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0].agent, "codex");
    assert_eq!(
        limits[0].windows,
        [window(300, 12.5, Some("3000-01-01T00:00:00.000Z"))]
    );
}

#[test]
fn a_window_that_has_reset_is_left_out() {
    let limits = [AgentLimits {
        agent: "codex".into(),
        windows: vec![
            window(300, 99.0, Some("2026-10-27T08:00:00.000Z")),
            window(10_080, 20.0, Some(LATER)),
        ],
        seen_at: String::new(),
    }];
    let now = 1_793_088_000_000; // 2026-10-27T08:00:00Z
    let current = current_limits(&limits, now);
    assert_eq!(current[0].windows, [window(10_080, 20.0, Some(LATER))]);
    let all_reset = current_limits(&limits, i64::MAX);
    assert!(all_reset.is_empty());
}

#[test]
fn a_window_without_a_reset_lasts_its_length_from_when_it_was_told() {
    let limits = [AgentLimits {
        agent: "claude-code".into(),
        windows: vec![window(300, 50.0, None)],
        seen_at: "2026-10-27T08:00:00.000Z".into(),
    }];
    let told = 1_793_088_000_000;
    assert_eq!(current_limits(&limits, told + 299 * 60_000).len(), 1);
    assert!(current_limits(&limits, told + 300 * 60_000).is_empty());
}

#[test]
fn an_older_window_does_not_replace_a_newer_one() {
    let known = [window(300, 30.0, Some("2026-10-27T13:00:00.000Z"))];
    // A session read late tells the window before the reset.
    let older = vec![window(300, 95.0, Some("2026-10-27T08:00:00.000Z"))];
    assert_eq!(merge_windows(&known, older), known);
    // The same window: the higher share.
    let same = vec![window(300, 20.0, Some("2026-10-27T13:00:00.000Z"))];
    assert_eq!(merge_windows(&known, same), known);
    // A later window replaces it.
    let later = vec![window(300, 5.0, Some("2026-10-27T18:00:00.000Z"))];
    assert_eq!(merge_windows(&known, later.clone()), later);
}
