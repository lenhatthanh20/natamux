use std::path::PathBuf;

use super::*;
use crate::security::{BAD_FILE_WARNING, SecurityMode, StrictSettings};

fn folder(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("natamux-security-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

const STRICT: &str = r#"{"strict": true}"#;
const OFF: &str = r#"{"strict": false}"#;

#[test]
fn a_missing_file_is_personal_without_a_warning() {
    let root = folder("missing");
    let state = Source::open(&root).current();
    assert_eq!(state.mode, SecurityMode::Personal);
    assert_eq!(state.warning, None);
}

#[test]
fn a_save_is_read_back_without_a_warning_and_turning_strict_off_works() {
    let root = folder("save");
    let source = Source::open(&root);
    let saved = source.write(STRICT).unwrap();
    assert!(saved.mode.is_strict() && saved.warning.is_none());
    assert_eq!(
        std::fs::read_to_string(root.join(FILE_NAME)).unwrap(),
        STRICT
    );
    assert!(source.generation() > 0);
    // A new start reads the same state: the copy matches the file.
    assert_eq!(Source::open(&root).current(), saved);
    let off = source.write(OFF).unwrap();
    assert_eq!(off.mode, SecurityMode::Personal);
    assert_eq!(Source::open(&root).current().warning, None);
}

#[cfg(unix)]
#[test]
fn the_saved_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let root = folder("modes");
    Source::open(&root).write(STRICT).unwrap();
    for name in [FILE_NAME, WRITTEN_NAME] {
        let mode = std::fs::metadata(root.join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}");
    }
}

#[test]
fn an_outside_edit_while_running_keeps_the_stricter_state_and_warns() {
    let root = folder("running");
    let source = Source::open(&root);
    source.write(STRICT).unwrap();
    let before = source.generation();
    std::fs::write(root.join(FILE_NAME), OFF).unwrap();
    let after = source.check_now();
    assert!(after > before);
    let state = source.current();
    assert_eq!(state.mode, SecurityMode::Strict(StrictSettings::default()));
    assert_eq!(state.warning.as_deref(), Some(OUTSIDE_CHANGE_WARNING));
    // Deleting the file does not turn Strict off either.
    std::fs::remove_file(root.join(FILE_NAME)).unwrap();
    source.check_now();
    assert!(source.current().mode.is_strict());
    // A save through the daemon clears the warning.
    let saved = source.write(OFF).unwrap();
    assert_eq!(saved, SecurityState::default());
}

#[test]
fn an_outside_edit_found_at_start_keeps_the_stricter_state_and_warns() {
    let root = folder("start");
    Source::open(&root).write(STRICT).unwrap();
    std::fs::write(root.join(FILE_NAME), OFF).unwrap();
    let state = Source::open(&root).current();
    assert!(state.mode.is_strict());
    assert_eq!(state.warning.as_deref(), Some(OUTSIDE_CHANGE_WARNING));
    // A file written by hand, with no copy of a save, is used and warned about.
    let by_hand = folder("by-hand");
    std::fs::write(by_hand.join(FILE_NAME), STRICT).unwrap();
    let state = Source::open(&by_hand).current();
    assert!(state.mode.is_strict());
    assert_eq!(state.warning.as_deref(), Some(OUTSIDE_CHANGE_WARNING));
}

#[test]
fn a_broken_saved_file_fails_closed() {
    let root = folder("broken");
    let state = Source::open(&root).write("{").unwrap();
    assert!(state.mode.is_strict());
    assert_eq!(state.warning.as_deref(), Some(BAD_FILE_WARNING));
}

#[test]
fn reads_again_at_most_once_per_second() {
    let root = folder("pace");
    let source = Source::open(&root);
    std::fs::write(root.join(FILE_NAME), STRICT).unwrap();
    // Right after the start the old state is kept; a forced check sees the change.
    assert_eq!(source.current().mode, SecurityMode::Personal);
    source.check_now();
    assert!(source.current().mode.is_strict());
}

#[test]
fn evaluate_tightens_only_when_the_file_and_the_copy_differ() {
    let strict = Some(STRICT.as_bytes());
    let off = Some(OFF.as_bytes());
    assert!(evaluate(strict, strict).warning.is_none());
    assert_eq!(evaluate(off, off).mode, SecurityMode::Personal);
    assert!(evaluate(off, strict).mode.is_strict());
    assert!(evaluate(None, strict).mode.is_strict());
    assert!(evaluate(None, None).warning.is_none());
}
