//! The group 'sessionFile': only the case that tests the service (see `main.rs`).

use super::*;

#[test]
fn is_not_read_when_refused() {
    let fx = Fx::new();
    let outside = fx.root.0.join("outside.jsonl");
    std::fs::write(&outside, opus_line("m1", 100, 10)).unwrap();

    fx.read(Some("implement"), "claude-code", &outside);

    assert!(fx.service.usage_of("T-0001").is_none());
}
