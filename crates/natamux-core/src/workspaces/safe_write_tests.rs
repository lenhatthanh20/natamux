use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static DIRS: AtomicU64 = AtomicU64::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        let path = std::env::temp_dir().join(format!(
            "natamux-safe-write-{}-{}",
            std::process::id(),
            DIRS.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names(folder: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn makes_the_folders_and_writes_the_file() {
    let dir = Dir::new();
    write_inside(&dir.0, Path::new("a/b/c.md"), b"one").unwrap();
    write_inside(&dir.0, Path::new("a/b/c.md"), b"two").unwrap();
    assert_eq!(fs::read(dir.0.join("a/b/c.md")).unwrap(), b"two");
    assert_eq!(names(&dir.0.join("a/b")), ["c.md"]);
}

#[test]
fn refuses_a_path_that_leaves_the_folder() {
    let dir = Dir::new();
    for bad in ["../x", "/tmp/x", "a/../../x", ""] {
        let error = write_inside(&dir.0, Path::new(bad), b"x").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad:?}");
    }
}

#[test]
fn copies_a_file() {
    let dir = Dir::new();
    fs::write(dir.0.join("from.txt"), b"data").unwrap();
    copy_inside(&dir.0, Path::new("to/x.txt"), &dir.0.join("from.txt")).unwrap();
    assert_eq!(fs::read(dir.0.join("to/x.txt")).unwrap(), b"data");
}

#[cfg(unix)]
mod links {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn setup() -> (Dir, PathBuf, PathBuf) {
        let dir = Dir::new();
        let work = dir.0.join("work");
        let outside = dir.0.join("outside");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&outside).unwrap();
        (dir, work, outside)
    }

    #[test]
    fn refuses_a_link_as_a_folder() {
        let (_dir, work, outside) = setup();
        symlink(&outside, work.join(".natamux-task")).unwrap();
        let error = write_inside(&work, Path::new(".natamux-task/spec.md"), b"x").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains(".natamux-task"));
        assert!(names(&outside).is_empty());
    }

    #[test]
    fn refuses_a_dangling_link_as_a_folder() {
        let (_dir, work, outside) = setup();
        symlink(outside.join("missing"), work.join("d")).unwrap();
        let error = write_inside(&work, Path::new("d/spec.md"), b"x").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(names(&outside).is_empty());
    }

    #[test]
    fn refuses_a_link_as_the_target() {
        let (_dir, work, outside) = setup();
        fs::write(outside.join("secret"), b"keep").unwrap();
        symlink(outside.join("secret"), work.join("spec.md")).unwrap();
        let error = write_inside(&work, Path::new("spec.md"), b"x").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read(outside.join("secret")).unwrap(), b"keep");
        assert_eq!(names(&work), ["spec.md"]);
    }

    #[test]
    fn refuses_a_dangling_link_as_the_target() {
        let (_dir, work, outside) = setup();
        symlink(outside.join("new"), work.join("spec.md")).unwrap();
        assert!(write_inside(&work, Path::new("spec.md"), b"x").is_err());
        assert!(names(&outside).is_empty());
    }

    #[test]
    fn a_hard_link_is_replaced_not_written_through() {
        let (_dir, work, outside) = setup();
        fs::write(outside.join("secret"), b"keep").unwrap();
        fs::hard_link(outside.join("secret"), work.join("spec.md")).unwrap();
        write_inside(&work, Path::new("spec.md"), b"new").unwrap();
        assert_eq!(fs::read(outside.join("secret")).unwrap(), b"keep");
        assert_eq!(fs::read(work.join("spec.md")).unwrap(), b"new");
    }

    #[test]
    fn keeps_the_mode_and_leaves_no_temp_file() {
        let (_dir, work, _outside) = setup();
        fs::write(work.join("run.sh"), b"a").unwrap();
        fs::set_permissions(work.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        write_inside(&work, Path::new("run.sh"), b"b").unwrap();
        let mode = fs::metadata(work.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(names(&work), ["run.sh"]);
    }

    #[test]
    fn leaves_a_file_with_the_same_text_alone() {
        use std::os::unix::fs::MetadataExt;
        let (_dir, work, _outside) = setup();
        write_inside(&work, Path::new("hooks.json"), b"same").unwrap();
        let before = fs::metadata(work.join("hooks.json")).unwrap().ino();
        write_inside(&work, Path::new("hooks.json"), b"same").unwrap();
        assert_eq!(fs::metadata(work.join("hooks.json")).unwrap().ino(), before);
        write_inside(&work, Path::new("hooks.json"), b"new").unwrap();
        assert_ne!(fs::metadata(work.join("hooks.json")).unwrap().ino(), before);
        assert_eq!(fs::read(work.join("hooks.json")).unwrap(), b"new");
    }

    #[test]
    fn a_failed_write_removes_its_temp_file() {
        let (_dir, work, _outside) = setup();
        fs::create_dir(work.join("spec.md")).unwrap();
        assert!(write_inside(&work, Path::new("spec.md"), b"x").is_err());
        assert_eq!(names(&work), ["spec.md"]);
    }
}
