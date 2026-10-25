use super::*;
use crate::test_support::TempDir;

struct Setup {
    root: TempDir,
    home: PathBuf,
    session: PathBuf,
}

fn setup() -> Setup {
    let root = TempDir::new("session-file");
    let home = root.path().join("home");
    fs::create_dir_all(home.join(".claude")).unwrap();
    let session = home.join(".claude").join("session.jsonl");
    Setup {
        root,
        home,
        session,
    }
}

fn find(path: &Path, home: &Path) -> Option<PathBuf> {
    session_file(&path.to_string_lossy(), home)
}

#[test]
fn takes_a_session_file_in_the_home_folder() {
    let s = setup();
    fs::write(&s.session, "").unwrap();
    assert!(find(&s.session, &s.home).is_some());
}

#[test]
fn refuses_a_path_outside_the_home_folder_or_not_to_a_session_file() {
    let s = setup();
    let outside = s.root.path().join("outside.jsonl");
    fs::write(&outside, "").unwrap();
    let text = s.home.join("notes.txt");
    fs::write(&text, "").unwrap();
    let folder = s.home.join("folder.jsonl");
    fs::create_dir(&folder).unwrap();

    assert_eq!(find(&outside, &s.home), None);
    assert_eq!(find(&text, &s.home), None);
    assert_eq!(find(&folder, &s.home), None);
    assert_eq!(session_file(".claude/session.jsonl", &s.home), None);
    assert_eq!(find(&s.home.join("missing.jsonl"), &s.home), None);
}

// Windows makes links only with more rights.
#[cfg(unix)]
#[test]
fn refuses_a_link_to_a_file_outside() {
    let s = setup();
    let outside = s.root.path().join("outside.jsonl");
    fs::write(&outside, "").unwrap();
    let link = s.home.join("link.jsonl");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    assert_eq!(find(&link, &s.home), None);
}

#[test]
fn refuses_a_file_over_200_mb() {
    let s = setup();
    let file = fs::File::create(&s.session).unwrap();
    file.set_len(200 * 1024 * 1024 + 1).unwrap();

    assert_eq!(find(&s.session, &s.home), None);
}
