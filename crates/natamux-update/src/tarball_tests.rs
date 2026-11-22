use super::tarball::{
    BINARIES, check_unpacked, is_tarball_layout, replace_folder, stale_tarball, unpack_tarball,
};
use std::fs;
use std::path::{Path, PathBuf};

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("natamux-tarball-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("folder");
    path
}

fn write_all(dir: &Path, text: &str) {
    for name in BINARIES {
        fs::write(dir.join(name), text).expect("binary");
    }
    fs::write(dir.join("natamux.desktop"), text).expect("desktop");
}

#[test]
fn the_layout_needs_the_daemon_and_the_desktop_file() {
    let dir = folder("layout");
    fs::write(dir.join("natamux-daemon"), "d").expect("daemon");
    assert!(!is_tarball_layout(&dir));
    fs::write(dir.join("natamux.desktop"), "d").expect("desktop");
    assert!(is_tarball_layout(&dir));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn the_unpack_check_refuses_folders_links_and_missing_binaries() {
    let dir = folder("check");
    write_all(&dir, "new");
    check_unpacked(&dir).expect("complete archive");

    fs::create_dir(dir.join("sub")).expect("sub");
    assert!(check_unpacked(&dir).is_err());
    fs::remove_dir(dir.join("sub")).expect("sub gone");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", dir.join("link")).expect("link");
        assert!(check_unpacked(&dir).is_err());
        fs::remove_file(dir.join("link")).expect("link gone");
    }

    fs::remove_file(dir.join("natamux-daemon")).expect("daemon gone");
    let error = check_unpacked(&dir).expect_err("no daemon");
    assert!(error.to_string().contains("natamux-daemon"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn a_real_archive_is_unpacked_and_checked() {
    let source = folder("source");
    write_all(&source, "new");
    let work = folder("archive");
    let archive = work.join("Natamux-linux-x64.tar.gz");
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg(".")
        .status()
        .expect("tar");
    assert!(status.success());
    let staged = work.join("tar");
    fs::create_dir_all(&staged).expect("old staged");
    fs::write(staged.join("leftover"), "x").expect("leftover");
    unpack_tarball(&archive, &staged).expect("unpack");
    assert_eq!(fs::read_to_string(staged.join("nmx")).expect("nmx"), "new");
    assert!(!staged.join("leftover").exists());
    fs::remove_dir_all(source).expect("cleanup");
    fs::remove_dir_all(work).expect("cleanup");
}

#[test]
fn replace_folder_swaps_staged_files_and_keeps_the_others() {
    let staged = folder("swap-staged");
    let target = folder("swap-target");
    write_all(&staged, "new");
    write_all(&target, "old");
    fs::write(target.join("notes.txt"), "mine").expect("user file");
    replace_folder(&staged, &target).expect("swap");
    for name in BINARIES {
        assert_eq!(
            fs::read_to_string(target.join(name)).expect("binary"),
            "new"
        );
    }
    assert_eq!(
        fs::read_to_string(target.join("notes.txt")).expect("user file"),
        "mine"
    );
    assert_eq!(stale_tarball(&staged, &target), None);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |name: &str| {
            fs::metadata(target.join(name))
                .expect("meta")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("natamux-app"), 0o755);
        assert_eq!(mode("natamux.desktop"), 0o644);
    }
    fs::remove_dir_all(staged).expect("cleanup");
    fs::remove_dir_all(target).expect("cleanup");
}

#[test]
fn a_failed_copy_leaves_the_old_files_and_no_new_files() {
    let staged = folder("fail-staged");
    let target = folder("fail-target");
    write_all(&staged, "new");
    // Sorted last, and a folder cannot be copied as a file.
    fs::create_dir(staged.join("zz")).expect("bad entry");
    write_all(&target, "old");
    assert!(replace_folder(&staged, &target).is_err());
    for name in BINARIES {
        assert_eq!(
            fs::read_to_string(target.join(name)).expect("binary"),
            "old"
        );
    }
    let leftovers = fs::read_dir(&target)
        .expect("list")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".natamux-new")
        })
        .count();
    assert_eq!(leftovers, 0);
    fs::remove_dir_all(staged).expect("cleanup");
    fs::remove_dir_all(target).expect("cleanup");
}

#[test]
fn an_old_new_file_blocks_the_swap() {
    let staged = folder("stale-staged");
    let target = folder("stale-target");
    write_all(&staged, "new");
    write_all(&target, "old");
    let stale = target.join("nmx.natamux-new");
    fs::write(&stale, "half").expect("stale");
    assert_eq!(stale_tarball(&staged, &target), Some(stale.clone()));
    let error = replace_folder(&staged, &target).expect_err("blocked");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(target.join("nmx")).expect("nmx"), "old");
    assert_eq!(fs::read_to_string(&stale).expect("stale"), "half");
    fs::remove_dir_all(staged).expect("cleanup");
    fs::remove_dir_all(target).expect("cleanup");
}
