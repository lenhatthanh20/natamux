//! The seen lists of the view state, and its parts about background work (T-0390).

use super::*;

#[test]
fn the_background_settings_are_off_by_default() {
    let fresh = ViewState::parse("{}");
    assert!(!fresh.notify_background_results);
    assert_eq!(fresh.flag_background_after_minutes, 0);
    assert!(fresh.seen_results.is_empty());
}

#[test]
fn keeps_the_background_settings_and_the_seen_results() {
    let state = ViewState {
        notify_background_results: true,
        flag_background_after_minutes: 60,
        seen_results: vec![
            ("T-1".to_owned(), "2026-11-26T09:00:00.000Z".to_owned()),
            ("T-2".to_owned(), String::new()),
        ],
        ..ViewState::default()
    };

    assert_eq!(ViewState::parse(&state.to_json()), state);
}

#[test]
fn a_bad_value_is_the_default() {
    let state = ViewState::parse(
        r#"{"notifyBackgroundResults": "yes", "flagBackgroundAfterMinutes": -5, "seenResults": {"T-1": 3}}"#,
    );
    assert!(!state.notify_background_results);
    assert_eq!(state.flag_background_after_minutes, 0);
    assert!(state.seen_results.is_empty());
}

#[test]
fn keeps_the_newest_300_folds_and_1000_seen_states() {
    let folds: Map<String, Value> = (0..310)
        .map(|at| (format!("task:T-{at}"), Value::Bool(true)))
        .collect();
    let seen: Map<String, Value> = (0..1010)
        .map(|at| (format!("T-{at}"), Value::from("done")))
        .collect();
    let text = json!({"folds": folds, "seen": seen}).to_string();
    let state = ViewState::parse(&text);
    assert_eq!(state.folds.len(), 300);
    assert_eq!(state.folds[0].0, "task:T-10");
    assert_eq!(state.seen.len(), 1000);
    assert_eq!(state.seen[0].0, "T-10");
}
