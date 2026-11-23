use super::*;

fn temp(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("natamux-private-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

#[test]
fn creates_the_folder_and_its_parents() {
    let path = temp("new").join("a/b");
    private_dir(&path).unwrap();
    assert!(path.is_dir());
    let _ = std::fs::remove_dir_all(temp("new"));
}

#[cfg(unix)]
#[test]
fn a_new_folder_is_0700() {
    use std::os::unix::fs::PermissionsExt;
    let path = temp("mode");
    private_dir(&path).unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o700);
    let _ = std::fs::remove_dir_all(&path);
}

#[cfg(unix)]
#[test]
fn a_wide_folder_is_tightened_and_its_files_stay() {
    use std::os::unix::fs::PermissionsExt;
    let path = temp("wide");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(path.join("keep"), b"x").unwrap();
    private_dir(&path).unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o700);
    assert!(path.join("keep").is_file());
    let _ = std::fs::remove_dir_all(&path);
}

#[cfg(unix)]
#[test]
fn write_private_makes_a_0600_file_also_over_a_wider_one_and_a_rename_keeps_it() {
    use std::os::unix::fs::PermissionsExt;
    let folder = temp("file");
    std::fs::create_dir_all(&folder).unwrap();
    let temporary = folder.join("a.tmp");
    std::fs::write(&temporary, b"old").unwrap();
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o644)).unwrap();
    write_private(&temporary, b"new").unwrap();
    std::fs::rename(&temporary, folder.join("a")).unwrap();
    let meta = folder.join("a").metadata().unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::read(folder.join("a")).unwrap(), b"new");
    let _ = std::fs::remove_dir_all(&folder);
}

#[cfg(unix)]
#[test]
fn a_log_line_is_appended_redacted_to_a_private_file() {
    use std::os::unix::fs::PermissionsExt;
    let folder = temp("log");
    let path = folder.join("logs/app.log");
    append_log_line(&path, "a https://u:FAKE-PASS-123@h.test/x").unwrap();
    append_log_line(&path, "b").unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "a https://h.test/x\nb\n"
    );
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    let logs = folder.join("logs").metadata().unwrap();
    assert_eq!(logs.permissions().mode() & 0o777, 0o700);
    let _ = std::fs::remove_dir_all(&folder);
}
