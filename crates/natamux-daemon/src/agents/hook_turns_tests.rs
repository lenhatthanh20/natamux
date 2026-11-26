use serde_json::{Map, json};

use super::*;

fn hook(signal: Signal, details: Value) -> NormalizedHook {
    let details: Map<String, Value> = match details {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    NormalizedHook {
        signal,
        cwd: None,
        details,
        summary: None,
        transcript: None,
    }
}

/// Notes the hook in shell "1" of T-1; gives the signal for the idle pause and the details.
fn note(turns: &mut Turns, signal: Signal, details: Value) -> (Signal, Map<String, Value>) {
    let mut hook = hook(signal, details);
    let pause = turns.note("T-1", "1", &mut hook);
    (pause, hook.details)
}

fn marked(details: &Map<String, Value>) -> bool {
    details.get(BACKGROUND_RESULT_KEY) == Some(&Value::Bool(true))
}

fn paused(turns: &mut Turns) -> Map<String, Value> {
    note(turns, Signal::TurnPaused, json!({})).1
}

#[test]
fn marks_a_paused_turn_that_no_prompt_started() {
    let mut turns = Turns::default();
    assert!(!marked(&paused(&mut turns)));
    note(&mut turns, Signal::Activity, json!({}));

    assert!(marked(&paused(&mut turns)));
}

#[test]
fn marks_a_turn_that_a_prompt_of_background_work_started() {
    let mut turns = Turns::default();
    paused(&mut turns);
    note(&mut turns, Signal::Prompt, json!({ WAKE_KEY: BACKGROUND }));

    assert!(marked(&paused(&mut turns)));
}

#[test]
fn does_not_mark_a_turn_that_the_user_started() {
    let mut turns = Turns::default();
    paused(&mut turns);
    note(
        &mut turns,
        Signal::Prompt,
        json!({ PROMPT_HEAD_KEY: "fix the test" }),
    );

    assert!(!marked(&paused(&mut turns)));
}

#[test]
fn does_not_mark_the_first_turn_end_it_hears_of() {
    let mut turns = Turns::default();
    assert!(!marked(&paused(&mut turns)));
}

#[test]
fn does_not_mark_a_turn_after_a_plain_turn_end() {
    let mut turns = Turns::default();
    note(&mut turns, Signal::TurnEnded, json!({}));

    assert!(!marked(&paused(&mut turns)));
}

#[test]
fn does_not_mark_the_last_turn_that_leaves_nothing() {
    let mut turns = Turns::default();
    paused(&mut turns);

    let (pause, details) = note(&mut turns, Signal::TurnEnded, json!({}));

    assert!(!marked(&details));
    assert_eq!(pause, Signal::TurnEnded);
}

#[test]
fn does_not_mark_a_question() {
    let mut turns = Turns::default();
    paused(&mut turns);

    let (_, details) = note(&mut turns, Signal::TurnAsked, json!({}));

    assert!(!marked(&details));
}

#[test]
fn tells_a_scheduled_prompt_from_a_prompt_of_the_user() {
    let mut turns = Turns::default();
    let crons = json!({ SCHEDULED_KEY: ["check CI"] });
    let (pause, details) = note(&mut turns, Signal::TurnEnded, crons.clone());
    assert_eq!(pause, Signal::TurnPaused);
    assert!(!marked(&details));

    let (_, prompt) = note(
        &mut turns,
        Signal::Prompt,
        json!({ PROMPT_HEAD_KEY: "check CI" }),
    );
    assert!(prompt.get(PROMPT_HEAD_KEY).is_none());
    let (pause, details) = note(&mut turns, Signal::TurnEnded, crons.clone());
    assert_eq!(pause, Signal::TurnPaused);
    assert!(marked(&details));

    note(
        &mut turns,
        Signal::Prompt,
        json!({ PROMPT_HEAD_KEY: "and now?" }),
    );
    let (_, details) = note(&mut turns, Signal::TurnEnded, crons);
    assert!(!marked(&details));
}

#[test]
fn keeps_shells_apart_and_forgets_an_ended_one() {
    let mut turns = Turns::default();
    paused(&mut turns);
    let mut other = hook(Signal::TurnPaused, json!({}));
    turns.note("T-1", "2", &mut other);
    assert!(!marked(&other.details));

    turns.forget("T-1", "1");

    assert!(!marked(&paused(&mut turns)));
}

#[test]
fn a_failure_ends_what_it_knew() {
    let mut turns = Turns::default();
    paused(&mut turns);
    note(&mut turns, Signal::Failed, json!({}));

    assert!(!marked(&paused(&mut turns)));
}
