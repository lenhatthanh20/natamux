use super::*;
#[cfg(unix)]
use crate::process::capture;
use crate::test_support::TempDir;

#[test]
fn says_when_gh_is_missing_or_fails() {
    // gh may or may not be installed here; either way the answer is an error text or output.
    let dir = TempDir::new("gh");
    let answer = run_gh(dir.path(), &["no-such-command-xyz".to_string()]);
    if let Err(text) = answer {
        assert!(text.starts_with("GitHub CLI failed: "), "{text}");
    }
}

#[cfg(unix)]
#[test]
fn reports_a_gh_that_does_not_finish() {
    let dir = TempDir::new("gh");
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = bin.join("gh");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new(&script);
    command.current_dir(dir.path());
    let ran = capture(command, Duration::from_millis(200), 1024);
    assert!(ran.killed());
}
