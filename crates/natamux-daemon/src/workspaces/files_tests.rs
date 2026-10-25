use std::fs::{create_dir, create_dir_all, read_to_string, write};

use natamux_git::test_support::TempDir;

use super::*;

struct Setup {
    base: TempDir,
    root: PathBuf,
}

fn setup() -> Setup {
    let base = TempDir::new("files");
    let root = base.path().join("worktree");
    create_dir(&root).unwrap();
    Setup { base, root }
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).unwrap();
}

fn sha(bytes: &[u8]) -> String {
    version_of(bytes)
}

fn entry(name: &str, kind: EntryKind) -> TreeEntry {
    TreeEntry {
        name: name.to_string(),
        kind,
    }
}

fn folder(name: &str) -> TreeEntry {
    entry(name, EntryKind::Folder)
}

fn file(name: &str) -> TreeEntry {
    entry(name, EntryKind::File)
}

fn list(root: &Path, path: &str) -> WorkspaceResult<Vec<TreeEntry>> {
    list_folder(root, path)
}

fn outside<T: std::fmt::Debug>(result: WorkspaceResult<T>) -> bool {
    matches!(result, Err(WorkspaceError::Outside(_)))
}

#[test]
fn lists_folders_first_then_files_by_name_with_numbers_in_order() {
    let s = setup();
    create_dir(s.root.join("src")).unwrap();
    create_dir(s.root.join("docs")).unwrap();
    for name in ["b10.ts", "b2.ts", ".env"] {
        write(s.root.join(name), "").unwrap();
    }

    assert_eq!(
        list(&s.root, "").unwrap(),
        [
            folder("docs"),
            folder("src"),
            file(".env"),
            file("b2.ts"),
            file("b10.ts")
        ]
    );
}

#[test]
fn lists_one_folder_only() {
    let s = setup();
    create_dir_all(s.root.join("src").join("deep")).unwrap();
    write(s.root.join("src").join("a.ts"), "").unwrap();
    write(s.root.join("src").join("deep").join("b.ts"), "").unwrap();

    assert_eq!(
        list(&s.root, "src").unwrap(),
        [folder("deep"), file("a.ts")]
    );
}

#[test]
fn never_lists_git_as_a_folder_or_as_the_file_of_a_worktree() {
    let s = setup();
    create_dir(s.root.join(".git")).unwrap();
    write(s.root.join("a.ts"), "").unwrap();
    create_dir(s.root.join("sub")).unwrap();
    write(s.root.join("sub").join(".git"), "gitdir: ../x").unwrap();

    assert_eq!(list(&s.root, "").unwrap(), [folder("sub"), file("a.ts")]);
    assert_eq!(list(&s.root, "sub").unwrap(), Vec::<TreeEntry>::new());
}

#[cfg(unix)]
#[test]
fn lists_a_link_by_what_it_points_to_and_leaves_out_a_link_that_leaves_the_worktree() {
    let s = setup();
    create_dir(s.root.join("src")).unwrap();
    write(s.root.join("a.ts"), "").unwrap();
    write(s.base.path().join("secret.txt"), "secret").unwrap();
    link(&s.root.join("src"), &s.root.join("src-link"));
    link(&s.root.join("a.ts"), &s.root.join("a-link.ts"));
    link(&s.base.path().join("secret.txt"), &s.root.join("out.txt"));
    link(&s.root.join("gone"), &s.root.join("broken"));

    assert_eq!(
        list(&s.root, "").unwrap(),
        [
            folder("src"),
            folder("src-link"),
            file("a-link.ts"),
            file("a.ts")
        ]
    );
}

#[cfg(unix)]
#[test]
fn refuses_a_folder_outside_the_worktree() {
    let s = setup();
    create_dir(s.base.path().join("other")).unwrap();
    link(&s.base.path().join("other"), &s.root.join("other"));

    assert!(outside(list(&s.root, "..")));
    assert!(outside(list(&s.root, &s.base.path().to_string_lossy())));
    assert!(outside(list(&s.root, "other")));
}

#[test]
fn refuses_the_git_folder() {
    let s = setup();
    create_dir(s.root.join(".git")).unwrap();
    assert!(matches!(
        list(&s.root, ".git"),
        Err(WorkspaceError::GitFolder(_))
    ));
    assert!(matches!(
        list(&s.root, "src/../.git"),
        Err(WorkspaceError::GitFolder(_))
    ));
}

#[test]
fn says_when_the_folder_is_missing_or_is_a_file() {
    let s = setup();
    write(s.root.join("a.ts"), "").unwrap();
    assert!(matches!(
        list(&s.root, "nope"),
        Err(WorkspaceError::PathNotFound(_))
    ));
    assert!(matches!(
        list(&s.root, "a.ts"),
        Err(WorkspaceError::NotA { .. })
    ));
}

#[test]
fn keeps_a_byte_order_mark_so_a_save_writes_it_back() {
    let s = setup();
    write(s.root.join("bom.txt"), "\u{feff}hello").unwrap();
    assert_eq!(
        read_workspace_file(&s.root, "bom.txt").unwrap().text,
        Some("\u{feff}hello".to_string())
    );
}

#[test]
fn reads_the_text_with_its_sha256_as_the_version() {
    let s = setup();
    create_dir(s.root.join("src")).unwrap();
    write(s.root.join("src").join("a.ts"), "const a = 1\n").unwrap();

    assert_eq!(
        read_workspace_file(&s.root, "src/a.ts").unwrap(),
        WorkspaceFile {
            path: "src/a.ts".into(),
            text: Some("const a = 1\n".into()),
            binary: false,
            too_large: false,
            version: sha(b"const a = 1\n"),
        }
    );
}

#[test]
fn sends_no_text_for_a_binary_a_not_utf8_or_a_too_large_file() {
    let s = setup();
    write(s.root.join("image.bin"), [1u8, 0, 2]).unwrap();
    write(s.root.join("latin1.txt"), [0x63u8, 0x61, 0x66, 0xe9]).unwrap();
    write(
        s.root.join("big.txt"),
        "a".repeat(MAX_FILE_BYTES as usize + 1),
    )
    .unwrap();

    let latin = read_workspace_file(&s.root, "latin1.txt").unwrap();
    assert_eq!((latin.text, latin.binary), (None, true));
    let image = read_workspace_file(&s.root, "image.bin").unwrap();
    assert_eq!(
        (image.text, image.binary, image.too_large),
        (None, true, false)
    );
    let big = read_workspace_file(&s.root, "big.txt").unwrap();
    assert_eq!((big.text, big.binary, big.too_large), (None, false, true));
}

#[cfg(unix)]
#[test]
fn refuses_paths_outside_the_worktree_inside_git_missing_or_a_folder() {
    let s = setup();
    let secret = s.base.path().join("secret.txt");
    write(&secret, "secret").unwrap();
    link(&secret, &s.root.join("link.txt"));
    create_dir(s.root.join(".git")).unwrap();
    write(s.root.join(".git").join("config"), "").unwrap();
    create_dir(s.root.join("src")).unwrap();
    let read = |path: &str| read_workspace_file(&s.root, path);

    assert!(outside(read("../secret.txt")));
    assert!(outside(read("../not-there.txt")));
    assert!(outside(read(&secret.to_string_lossy())));
    assert!(outside(read("link.txt")));
    assert!(matches!(
        read(".git/config"),
        Err(WorkspaceError::GitFolder(_))
    ));
    assert!(matches!(
        read("nope.ts"),
        Err(WorkspaceError::PathNotFound(_))
    ));
    assert!(matches!(read("src"), Err(WorkspaceError::NotA { .. })));
}

#[cfg(unix)]
#[test]
fn refuses_a_link_that_points_into_git() {
    let s = setup();
    create_dir(s.root.join(".git")).unwrap();
    write(s.root.join(".git").join("config"), "").unwrap();
    link(&s.root.join(".git").join("config"), &s.root.join("config"));

    assert!(matches!(
        read_workspace_file(&s.root, "config"),
        Err(WorkspaceError::GitFolder(_))
    ));
}

#[cfg(unix)]
#[test]
fn writes_the_text_in_place_keeps_the_mode_and_answers_the_new_version() {
    use std::os::unix::fs::PermissionsExt;

    let s = setup();
    let path = s.root.join("run.sh");
    write(&path, "echo a\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let saved = write_workspace_file(&s.root, "run.sh", "echo b\n", &sha(b"echo a\n")).unwrap();

    assert_eq!(
        saved,
        SavedFile {
            path: "run.sh".into(),
            version: sha(b"echo b\n")
        }
    );
    assert_eq!(read_to_string(&path).unwrap(), "echo b\n");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
fn does_not_replace_a_file_that_changed_after_it_was_read() {
    let s = setup();
    let path = s.root.join("a.ts");
    write(&path, "agent text").unwrap();

    let result = write_workspace_file(&s.root, "a.ts", "mine", &sha(b"old text"));

    assert!(matches!(result, Err(WorkspaceError::ChangedOnDisk(_))));
    assert_eq!(read_to_string(&path).unwrap(), "agent text");
}

#[test]
fn refuses_a_text_over_the_size_limit() {
    let s = setup();
    write(s.root.join("a.ts"), "a").unwrap();
    let big = "a".repeat(MAX_FILE_BYTES as usize + 1);
    assert!(matches!(
        write_workspace_file(&s.root, "a.ts", &big, &sha(b"a")),
        Err(WorkspaceError::TooLarge(_))
    ));
}

#[test]
fn refuses_a_missing_file_a_folder_git_and_paths_outside() {
    let s = setup();
    let secret = s.base.path().join("secret.txt");
    write(&secret, "secret").unwrap();
    #[cfg(unix)]
    link(&secret, &s.root.join("link.txt"));
    create_dir(s.root.join(".git")).unwrap();
    write(s.root.join(".git").join("HEAD"), "ref").unwrap();
    create_dir(s.root.join("src")).unwrap();
    let put = |path: &str, version: &str| write_workspace_file(&s.root, path, "x", version);

    assert!(matches!(
        put("new.ts", &sha(b"")),
        Err(WorkspaceError::PathNotFound(_))
    ));
    assert!(matches!(
        put("src", &sha(b"")),
        Err(WorkspaceError::NotA { .. })
    ));
    assert!(matches!(
        put(".git/HEAD", &sha(b"ref")),
        Err(WorkspaceError::GitFolder(_))
    ));
    #[cfg(unix)]
    assert!(outside(put("link.txt", &sha(b"secret"))));
    assert!(outside(put("../secret.txt", &sha(b"secret"))));
    assert_eq!(read_to_string(&secret).unwrap(), "secret");
}
