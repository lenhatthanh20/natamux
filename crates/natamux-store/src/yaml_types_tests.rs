use serde_json::json;

use super::*;

fn value(text: &str) -> Plain {
    resolve_plain(text)
}

#[test]
fn reads_plain_scalars_as_js_yaml_3() {
    for (text, expected) in [
        ("~", json!(null)),
        ("Null", json!(null)),
        ("TRUE", json!(true)),
        ("yes", json!("yes")),
        ("12", json!(12)),
        ("1_000", json!(1000)),
        ("0x1F", json!(31)),
        ("017", json!(15)),
        ("0b101", json!(5)),
        ("1:30", json!(90)),
        ("-2", json!(-2)),
        ("1.5", json!(1.5)),
        ("1e3", json!(1000)),
        (".5", json!(0.5)),
        ("1:30.5", json!(90.5)),
        ("08", json!("08")),
        ("1_", json!("1_")),
        ("T-0001", json!("T-0001")),
        ("0o17", json!("0o17")),
    ] {
        assert_eq!(value(text), Plain::Value(expected), "{text:?}");
    }
    assert_eq!(value("<<"), Plain::Merge);
}

#[test]
fn reads_timestamps_as_dates_do() {
    for (text, iso) in [
        ("2026-10-08", "2026-10-08T00:00:00.000Z"),
        ("2026-10-08T09:00:00Z", "2026-10-08T09:00:00.000Z"),
        ("2026-10-08T09:00:00.1234Z", "2026-10-08T09:00:00.123Z"),
        ("2026-10-08 9:00:00 +07:00", "2026-10-08T02:00:00.000Z"),
        ("2026-1-8t09:00:00-2", "2026-01-08T11:00:00.000Z"),
        ("2026-10-08T09:00:00", "2026-10-08T09:00:00.000Z"),
        ("2026-13-01", "2027-01-01T00:00:00.000Z"),
    ] {
        assert_eq!(value(text), Plain::Timestamp(iso.into()), "{text:?}");
    }
    assert_eq!(
        value("2026-10-08T09:00"),
        Plain::Value(json!("2026-10-08T09:00"))
    );
    assert_eq!(
        value("2026-10-08T09:00:00 "),
        Plain::Value(json!("2026-10-08T09:00:00 "))
    );
}

#[test]
fn tells_which_strings_need_quotes() {
    for text in [
        "null",
        "true",
        "12",
        "1.5",
        ".inf",
        "-.INF",
        ".NaN",
        "2026-10-08",
        "<<",
        "0b1",
    ] {
        assert!(is_ambiguous(text), "{text:?}");
    }
    for text in ["T-0001", "1.2.3", "0o17", "yes", "+.5", "1__"] {
        assert!(!is_ambiguous(text), "{text:?}");
    }
}
