use super::*;

const MB: u64 = 1024 * 1024;

fn checking(manual: bool) -> UpdateState {
    UpdateState::Checking { manual }
}

fn downloading(manual: bool) -> UpdateState {
    UpdateState::Downloading {
        version: "1.0.6".into(),
        manual,
    }
}

fn ready() -> UpdateState {
    UpdateState::Ready {
        version: "1.0.6".into(),
    }
}

fn manual_release() -> UpdateState {
    UpdateState::Manual {
        version: "1.0.6".into(),
        page: "https://example.test/release".into(),
    }
}

fn failed() -> UpdateState {
    UpdateState::Failed {
        message: "The latest release could not be reached".into(),
    }
}

#[test]
fn a_manual_check_joins_a_running_one() {
    assert_eq!(checking(false).on_check(true), CheckStart::Join);
    assert_eq!(downloading(false).on_check(true), CheckStart::Join);
    assert_eq!(checking(false).joined(), checking(true));
    assert_eq!(downloading(false).joined(), downloading(true));
}

#[test]
fn a_timed_check_skips_a_running_one() {
    assert_eq!(checking(true).on_check(false), CheckStart::Skip);
    assert_eq!(downloading(true).on_check(false), CheckStart::Skip);
}

#[test]
fn a_ready_update_is_not_checked_or_downloaded_again() {
    assert_eq!(ready().on_check(false), CheckStart::Skip);
    assert_eq!(ready().on_check(true), CheckStart::ShowReady);
}

#[test]
fn other_states_run_a_check() {
    for state in [
        UpdateState::Idle,
        UpdateState::UpToDate,
        manual_release(),
        failed(),
    ] {
        assert_eq!(state.on_check(false), CheckStart::Run, "{state:?}");
        assert_eq!(state.on_check(true), CheckStart::Run, "{state:?}");
    }
}

#[test]
fn a_download_keeps_whether_the_check_was_manual() {
    assert_eq!(checking(true).downloading("1.0.6"), downloading(true));
    assert_eq!(checking(false).downloading("1.0.6"), downloading(false));
    assert!(downloading(true).manual());
    assert!(!downloading(false).manual());
    assert!(!ready().manual());
}

#[test]
fn only_a_check_or_a_download_is_busy() {
    assert!(checking(false).busy());
    assert!(downloading(false).busy());
    assert!(!ready().busy());
    assert!(!UpdateState::Idle.busy());
}

#[test]
fn the_status_item_of_each_state() {
    let item = |state: UpdateState| state.status_item(42 * MB);
    assert_eq!(item(UpdateState::Idle), None);
    assert_eq!(item(UpdateState::UpToDate), None);
    assert_eq!(item(checking(false)), None);
    assert_eq!(
        item(checking(true)),
        Some(StatusWords {
            kind: ItemKind::Busy,
            label: "Checking for updates\u{2026}".into(),
            tooltip: None,
        })
    );
    assert_eq!(
        item(downloading(false)),
        Some(StatusWords {
            kind: ItemKind::Busy,
            label: "Downloading 1.0.6 \u{b7} 42 MB".into(),
            tooltip: Some("Downloading Natamux 1.0.6".into()),
        })
    );
    assert_eq!(
        item(ready()),
        Some(StatusWords {
            kind: ItemKind::Restart,
            label: "Restart to update".into(),
            tooltip: Some("Natamux 1.0.6 is ready. Restart to install it.".into()),
        })
    );
    assert_eq!(
        item(manual_release()),
        Some(StatusWords {
            kind: ItemKind::OpenPage,
            label: "Natamux 1.0.6 available".into(),
            tooltip: Some("Open the release page to install it".into()),
        })
    );
    assert_eq!(
        item(failed()),
        Some(StatusWords {
            kind: ItemKind::Retry,
            label: "Update failed".into(),
            tooltip: Some("The latest release could not be reached. Click to try again.".into()),
        })
    );
}

#[test]
fn the_settings_line_of_each_state() {
    let line = |state: UpdateState, ago: Option<&str>| state.settings_line(42 * MB, ago);
    assert_eq!(line(UpdateState::Idle, None), "Not checked yet");
    assert_eq!(
        line(UpdateState::Idle, Some("3m ago")),
        "Last checked 3m ago"
    );
    assert_eq!(line(checking(false), None), "Checking for updates\u{2026}");
    assert_eq!(
        line(downloading(true), None),
        "Downloading 1.0.6 \u{b7} 42 MB"
    );
    assert_eq!(line(ready(), None), "1.0.6 is ready to install");
    assert_eq!(
        line(manual_release(), None),
        "1.0.6 is available on the release page"
    );
    assert_eq!(
        line(UpdateState::UpToDate, Some("just now")),
        "Up to date \u{b7} last checked just now"
    );
    assert_eq!(
        line(failed(), None),
        "Last check failed: The latest release could not be reached"
    );
}

#[test]
fn the_settings_button_follows_the_state() {
    assert_eq!(checking(true).check_button(), ("Checking\u{2026}", false));
    assert_eq!(
        downloading(false).check_button(),
        ("Checking\u{2026}", false)
    );
    assert_eq!(ready().check_button(), ("Restart to Update", true));
    assert_eq!(
        UpdateState::Idle.check_button(),
        ("Check now\u{2026}", true)
    );
    assert_eq!(failed().check_button(), ("Check now\u{2026}", true));
}

#[test]
fn sizes_and_ages_read_short() {
    assert_eq!(megabytes(0), "0 MB");
    assert_eq!(megabytes(42 * MB + MB / 2), "42 MB");
    assert_eq!(ago(5), "just now");
    assert_eq!(ago(4 * 60), "4m ago");
    assert_eq!(ago(3 * 3600), "3h ago");
    assert_eq!(ago(2 * 86_400), "2d ago");
}

fn stale() -> UpdateState {
    UpdateState::Failed {
        message: "An old update backup is in the way: /Applications/Natamux.app.old. Remove it, then try again."
            .into(),
    }
}

#[test]
fn a_stale_backup_shows_update_failed_with_its_path() {
    let item = stale().status_item(0).expect("item");
    assert_eq!(item.kind, ItemKind::Retry);
    assert_eq!(item.label, "Update failed");
    let tooltip = item.tooltip.expect("tooltip");
    assert!(tooltip.contains("/Applications/Natamux.app.old"));
    assert!(tooltip.ends_with("then try again. Click to try again."));
    assert!(!tooltip.contains(".."));
    assert!(
        stale()
            .settings_line(0, None)
            .contains("/Applications/Natamux.app.old")
    );
}
