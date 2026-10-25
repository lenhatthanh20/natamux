//! The group 'UsageService'.

use natamux_daemon::agents::usage_service::{UsageOptions, UsageService, usage_path};
use serde_json::Map;

use super::*;

fn close_to(value: &Value, expected: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|got| (got - expected).abs() < 1e-9)
}

#[test]
fn adds_a_session_to_the_stage_of_its_turn_and_sends_the_new_usage() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 1000, 1000)).unwrap();

    fx.read_session(Some("implement"));

    let usage = fx.service.usage_of("T-0001").unwrap();
    assert_eq!(usage["tokens"], 2000);
    assert!(close_to(&usage["costUsd"], 0.024), "{usage}");
    assert_eq!(usage["unpricedModels"], json!([]));
    let rows = usage["stages"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    for (field, expected) in [
        ("stage", json!("implement")),
        ("agent", json!("claude-code")),
        ("model", json!("claude-opus-5-5")),
        ("input", json!(1000)),
        ("cacheRead", json!(0)),
        ("cacheWrite", json!(0)),
        ("output", json!(1000)),
    ] {
        assert_eq!(row[field], expected, "{field}");
    }
    assert!(close_to(&row["costUsd"], 0.024));
    assert_eq!(
        fx.messages(),
        [DaemonMessage::UsageChanged {
            task_id: "T-0001".into(),
            usage: usage.clone(),
        }]
    );
    let mut all = Map::new();
    all.insert("T-0001".into(), usage);
    assert_eq!(fx.service.all(), all);
}

#[test]
fn gives_the_next_stage_only_what_came_after_the_last_read() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 100, 10)).unwrap();
    fx.read_session(Some("implement"));
    append(&fx.session, &opus_line("m2", 50, 5));

    fx.read_session(Some("review"));
    fx.read_session(Some("review"));

    let usage = fx.service.usage_of("T-0001").unwrap();
    assert_eq!(
        stage_inputs(&usage),
        [("implement".to_string(), 100), ("review".to_string(), 50)]
    );
    assert_eq!(fx.messages().len(), 2);
}

#[test]
fn puts_a_turn_on_no_stage_under_an_empty_name() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 100, 10)).unwrap();

    fx.read_session(None);

    let usage = fx.service.usage_of("T-0001").unwrap();
    assert_eq!(usage["stages"][0]["stage"], "");
}

#[test]
fn reads_a_file_again_after_a_read_that_ran_when_its_hook_came() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 100, 10)).unwrap();
    let file = fx.session.to_string_lossy().into_owned();
    fx.service
        .seen("T-0001", Some("implement"), "claude-code", &file);
    append(&fx.session, &opus_line("m2", 50, 5));
    fx.service
        .seen("T-0001", Some("implement"), "claude-code", &file);
    fx.service.settle();

    let usage = fx.service.usage_of("T-0001").unwrap();
    assert_eq!(usage["stages"][0]["input"], 150);
}

#[test]
fn lists_a_model_with_no_price_and_leaves_the_cost_of_its_rows_null() {
    let fx = Fx::new();
    std::fs::write(&fx.session, claude_line("m1", 100, 10, "mystery-model")).unwrap();

    fx.read_session(Some("implement"));

    let usage = fx.service.usage_of("T-0001").unwrap();
    assert_eq!(usage["tokens"], 110);
    assert_eq!(usage["costUsd"], Value::Null);
    assert_eq!(usage["unpricedModels"], json!(["mystery-model"]));
    assert_eq!(usage["stages"][0]["model"], "mystery-model");
    assert_eq!(usage["stages"][0]["costUsd"], Value::Null);
}

#[test]
fn reads_nothing_for_an_agent_that_writes_no_usage() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 100, 10)).unwrap();

    fx.read(Some("implement"), "cursor", &fx.session);

    assert!(fx.service.usage_of("T-0001").is_none());
}

#[test]
fn keeps_the_usage_over_a_restart_and_forgets_it_when_the_task_is_deleted() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 100, 10)).unwrap();
    fx.read_session(Some("implement"));

    let again = fx.restarted(true);
    assert_eq!(again.usage_of("T-0001"), fx.service.usage_of("T-0001"));

    fx.bus.publish(
        natamux_daemon::shared::event_bus::DaemonMessage::TaskDeleted {
            task_id: "T-0001".into(),
        },
    );
    assert!(again.usage_of("T-0001").is_none());
    assert!(again.all().is_empty());
}

#[test]
fn reads_a_broken_usage_file_as_no_usage() {
    let fx = Fx::new();
    std::fs::create_dir_all(fx.data.join("tasks")).unwrap();
    std::fs::write(usage_path(&fx.data, "T-0002"), r#"{"files":"#).unwrap();

    let service = UsageService::new(UsageOptions {
        data_dir: fx.data.clone(),
        bus: None,
        home: fx.home.clone(),
    });

    assert!(service.usage_of("T-0002").is_none());
}

#[test]
fn writes_the_usage_file() {
    let fx = Fx::new();
    std::fs::write(&fx.session, opus_line("m1", 100, 10)).unwrap();
    fx.read_session(Some("implement"));

    let path = usage_path(&fx.data, "T-0001");
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let session = fx.session.to_string_lossy().into_owned();
    assert_eq!(stored["files"][session.as_str()]["agent"], "claude-code");
    let models = &stored["stages"]["implement"]["claude-code"];
    assert_eq!(models["claude-opus-5-5"]["input"], 100);
    let temporary = format!("{}.tmp", path.to_string_lossy());
    assert!(!std::path::Path::new(&temporary).exists());
}
