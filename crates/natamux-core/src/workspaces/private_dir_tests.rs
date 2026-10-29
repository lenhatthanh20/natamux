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
