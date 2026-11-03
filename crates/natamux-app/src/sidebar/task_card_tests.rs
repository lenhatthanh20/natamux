use super::*;
use natamux_proto::tasks::task::TaskState;

use crate::sidebar::workspace_row::task_title_size;

#[test]
fn the_open_or_focused_card_has_the_accent_border() {
    assert_eq!(card_border(true, false, true), CardBorder::Accent);
    assert_eq!(card_border(false, true, false), CardBorder::Accent);
}

#[test]
fn an_attention_card_has_its_state_border_and_others_none() {
    assert_eq!(card_border(false, false, true), CardBorder::Attention);
    assert_eq!(card_border(false, false, false), CardBorder::Plain);
}

fn title_source<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(name)
        .unwrap_or_else(|| panic!("missing {name}"));
    source[start..]
        .split_once("\n}")
        .map_or_else(|| panic!("missing end of {name}"), |(body, _)| body)
}

#[test]
fn closed_task_titles_and_blurred_placeholders_use_regular_weight() {
    let title = title_source(
        include_str!("workspace_row.rs"),
        "pub(super) fn title_label(",
    );
    assert!(title.contains("let weight = FontWeight::NORMAL"));
    assert_eq!(title.matches(".weight(weight)").count(), 2);
    assert!(!title.contains("FontWeight::SEMIBOLD"));
    assert!(!title.contains("FontWeight::MEDIUM"));
    assert!(!title.contains("unseen"));
}

#[test]
fn open_task_titles_and_blurred_placeholders_use_regular_weight() {
    let title = title_source(
        include_str!("task_card_open.rs"),
        "pub(super) fn open_title(",
    );
    assert_eq!(
        title.matches(".weight(gpui::FontWeight::NORMAL)").count(),
        2
    );
    assert!(!title.contains("FontWeight::SEMIBOLD"));
    assert!(!title.contains("FontWeight::MEDIUM"));
    assert!(title.contains(".line_clamp(2)"));
}

#[test]
fn task_title_size_follows_the_task_tier() {
    for state in [
        TaskState::Running,
        TaskState::Ready,
        TaskState::NeedsInput,
        TaskState::Failed,
        TaskState::Blocked,
        TaskState::Review,
    ] {
        assert_eq!(task_title_size(state), 13.0);
    }
    for state in [TaskState::Done, TaskState::Cancelled, TaskState::Draft] {
        assert_eq!(task_title_size(state), 12.0);
    }
}
