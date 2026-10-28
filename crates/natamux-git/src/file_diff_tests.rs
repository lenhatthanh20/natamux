use super::*;
use crate::test_support::{TempDir, run_git, test_repo};
use crate::worktrees::add_worktree;

const BRANCH: &str = "natamux/t-0001-example";

fn change(path: &str, status: ChangeStatus, original_path: Option<&str>) -> ChangeEntry {
    ChangeEntry {
        path: path.to_string(),
        status,
        original_path: original_path.map(str::to_string),
        added: None,
        removed: None,
    }
}

// A worktree on a new branch, and the folder that holds it and the repository.
fn worktree() -> (TempDir, PathBuf) {
    let (root, repo) = test_repo();
    let path = root.path().join("worktrees").join("T-0001");
    add_worktree(&repo, &path, BRANCH, None, false).unwrap();
    (root, path)
}

fn commit_file(path: &Path, name: &str, content: &[u8], message: &str) {
    std::fs::write(path.join(name), content).unwrap();
    run_git(path, &["add", name]);
    run_git(path, &["commit", "-m", message]);
}

mod diff {
    use super::*;

    fn prepared() -> (TempDir, PathBuf) {
        let (root, path) = worktree();
        commit_file(&path, "tracked.txt", b"one\n", "add tracked");
        (root, path)
    }

    #[test]
    fn gives_the_last_commit_and_the_file_now_for_a_modified_file() {
        let (_root, path) = prepared();
        std::fs::write(path.join("tracked.txt"), "two\n").unwrap();

        let diff = read_file_diff(&path, &change("tracked.txt", ChangeStatus::Modified, None));

        assert_eq!(
            diff.unwrap(),
            FileDiff {
                path: "tracked.txt".to_string(),
                status: ChangeStatus::Modified,
                original: Some("one\n".to_string()),
                modified: Some("two\n".to_string()),
                binary: false,
                too_large: false,
            }
        );
    }

    #[test]
    fn has_no_original_for_a_file_that_is_not_tracked_yet() {
        let (_root, path) = prepared();
        std::fs::write(path.join("new.txt"), "new\n").unwrap();

        let entry = change("new.txt", ChangeStatus::Untracked, None);
        let diff = read_file_diff(&path, &entry).unwrap();

        assert_eq!(diff.original, None);
        assert_eq!(diff.modified.as_deref(), Some("new\n"));
    }

    #[test]
    fn has_no_modified_side_for_a_deleted_file() {
        let (_root, path) = prepared();
        std::fs::remove_file(path.join("tracked.txt")).unwrap();

        let diff =
            read_file_diff(&path, &change("tracked.txt", ChangeStatus::Deleted, None)).unwrap();

        assert_eq!(diff.original.as_deref(), Some("one\n"));
        assert_eq!(diff.modified, None);
    }

    #[test]
    fn takes_the_original_of_a_renamed_file_from_its_old_path() {
        let (_root, path) = prepared();
        run_git(&path, &["mv", "tracked.txt", "moved.txt"]);

        let entry = change("moved.txt", ChangeStatus::Renamed, Some("tracked.txt"));
        let diff = read_file_diff(&path, &entry).unwrap();

        assert_eq!(diff.original.as_deref(), Some("one\n"));
        assert_eq!(diff.modified.as_deref(), Some("one\n"));
    }

    #[test]
    fn does_not_send_the_text_of_a_binary_file() {
        let (_root, path) = prepared();
        std::fs::write(path.join("image.bin"), [1u8, 2, 0, 3]).unwrap();

        let entry = change("image.bin", ChangeStatus::Untracked, None);
        let diff = read_file_diff(&path, &entry).unwrap();

        assert!(diff.binary);
        assert_eq!((diff.original, diff.modified), (None, None));
    }

    #[test]
    fn does_not_send_a_file_that_is_too_large() {
        let (_root, path) = prepared();
        std::fs::write(path.join("big.txt"), "a".repeat(1_000_001)).unwrap();

        let entry = change("big.txt", ChangeStatus::Untracked, None);
        let diff = read_file_diff(&path, &entry).unwrap();

        assert!(diff.too_large);
        assert_eq!((diff.original, diff.modified), (None, None));
    }

    #[test]
    fn reads_the_last_commit_with_one_git_process() {
        let (_root, path) = prepared();
        std::fs::write(path.join("tracked.txt"), "two\n").unwrap();
        let entry = change("tracked.txt", ChangeStatus::Modified, None);

        let before = crate::exec::thread_spawn_count();
        let diff = read_file_diff(&path, &entry).unwrap();

        assert_eq!(crate::exec::thread_spawn_count() - before, 1);
        assert_eq!(diff.original.as_deref(), Some("one\n"));
        assert_eq!(diff.modified.as_deref(), Some("two\n"));
    }

    #[test]
    fn does_not_send_a_committed_file_that_is_too_large() {
        let (_root, path) = prepared();
        commit_file(
            &path,
            "big.txt",
            "a".repeat(1_000_001).as_bytes(),
            "add big",
        );
        let entry = change("big.txt", ChangeStatus::Modified, None);

        let before = crate::exec::thread_spawn_count();
        let diff = read_file_diff(&path, &entry).unwrap();

        assert_eq!(crate::exec::thread_spawn_count() - before, 1);
        assert!(diff.too_large);
        assert_eq!((diff.original, diff.modified), (None, None));
    }

    #[test]
    fn fails_for_a_file_that_is_not_in_the_last_commit() {
        let (_root, path) = prepared();
        let entry = change("nothing.txt", ChangeStatus::Modified, None);

        assert!(matches!(
            read_file_diff(&path, &entry),
            Err(FileDiffError::Git(_))
        ));
    }

    // Unix only: a symlink to a file needs a privilege on Windows.
    #[cfg(unix)]
    #[test]
    fn refuses_a_link_that_leaves_the_worktree() {
        let (root, path) = prepared();
        let secret = root.path().join("secret.txt");
        std::fs::write(&secret, "secret").unwrap();
        std::os::unix::fs::symlink(&secret, path.join("link.txt")).unwrap();

        let error = read_file_diff(&path, &change("link.txt", ChangeStatus::Untracked, None));

        assert!(matches!(error, Err(FileDiffError::Outside(_))), "{error:?}");
    }
}

mod side {
    use super::*;

    #[test]
    fn reads_the_new_side_from_the_worktree_and_the_old_side_from_the_last_commit() {
        let (_root, path) = worktree();
        commit_file(&path, "a.txt", b"one\n", "add a");
        std::fs::write(path.join("a.txt"), "two\n").unwrap();

        let new = read_side(&path, "a.txt", None, CommentSide::New).unwrap();
        let old = read_side(&path, "a.txt", None, CommentSide::Old).unwrap();

        assert_eq!(new.as_deref(), Some("two\n"));
        assert_eq!(old.as_deref(), Some("one\n"));
    }

    #[test]
    fn has_no_old_side_for_a_new_file_and_follows_a_rename() {
        let (_root, path) = worktree();
        commit_file(&path, "a.txt", b"one\n", "add a");
        run_git(&path, &["mv", "a.txt", "b.txt"]);
        std::fs::write(path.join("c.txt"), "new\n").unwrap();

        let renamed = read_side(&path, "b.txt", Some("a.txt"), CommentSide::Old).unwrap();
        let fresh = read_side(&path, "c.txt", None, CommentSide::Old).unwrap();
        let gone = read_side(&path, "none.txt", None, CommentSide::New).unwrap();

        assert_eq!(renamed.as_deref(), Some("one\n"));
        assert_eq!((fresh, gone), (None, None));
    }

    #[test]
    fn has_no_text_for_a_binary_file() {
        let (_root, path) = worktree();
        std::fs::write(path.join("x.bin"), [1u8, 0, 2]).unwrap();

        let text = read_side(&path, "x.bin", None, CommentSide::New).unwrap();

        assert_eq!(text, None);
    }
}

mod image {
    use super::*;

    const PNG_HEAD: [u8; 12] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d,
    ];

    fn png(last: u8) -> Vec<u8> {
        let mut bytes = PNG_HEAD.to_vec();
        bytes.push(last);
        bytes
    }

    fn read(
        path: &Path,
        name: &str,
        status: ChangeStatus,
        side: ImageSide,
    ) -> Result<(String, Vec<u8>), FileImageError> {
        read_file_image(path, &change(name, status, None), side)
    }

    fn prepared() -> (TempDir, PathBuf) {
        let (root, path) = worktree();
        commit_file(&path, "logo.png", &png(1), "add logo");
        (root, path)
    }

    #[test]
    fn gives_both_sides_of_a_modified_image() {
        let (_root, path) = prepared();
        std::fs::write(path.join("logo.png"), png(2)).unwrap();

        let (modified, original) = (ImageSide::Modified, ImageSide::Original);
        let before = read(&path, "logo.png", ChangeStatus::Modified, original).unwrap();
        let after = read(&path, "logo.png", ChangeStatus::Modified, modified).unwrap();

        assert_eq!(before, ("image/png".to_string(), png(1)));
        assert_eq!(after, ("image/png".to_string(), png(2)));
    }

    #[test]
    fn has_no_original_for_a_new_image_and_no_modified_side_for_a_deleted_one() {
        let (_root, path) = prepared();
        std::fs::write(path.join("new.png"), PNG_HEAD).unwrap();

        let new = read(&path, "new.png", ChangeStatus::Added, ImageSide::Original);
        let gone = read(
            &path,
            "logo.png",
            ChangeStatus::Deleted,
            ImageSide::Modified,
        );

        assert_eq!(
            new,
            Err(FileImageError::SideMissing(
                "new.png".to_string(),
                ImageSide::Original
            ))
        );
        assert_eq!(
            gone.unwrap_err().to_string(),
            "logo.png is not in the workspace any more"
        );
        assert_eq!(
            new.unwrap_err().to_string(),
            "new.png is not in the last commit"
        );
    }

    #[test]
    fn refuses_an_image_that_is_too_large() {
        let (_root, path) = prepared();
        let size = usize::try_from(MAX_IMAGE_BYTES).unwrap() + 1;
        std::fs::write(path.join("big.png"), vec![0u8; size]).unwrap();

        let error = read(
            &path,
            "big.png",
            ChangeStatus::Untracked,
            ImageSide::Modified,
        );

        assert_eq!(error, Err(FileImageError::TooLarge("big.png".to_string())));
        assert_eq!(
            error.unwrap_err().to_string(),
            "big.png is larger than 5 MB, so it is not shown"
        );
    }

    // Unix only: a symlink to a file needs a privilege on Windows.
    #[cfg(unix)]
    #[test]
    fn refuses_a_link_that_leaves_the_worktree() {
        let (root, path) = prepared();
        let secret = root.path().join("secret.png");
        std::fs::write(&secret, PNG_HEAD).unwrap();
        std::os::unix::fs::symlink(&secret, path.join("link.png")).unwrap();

        let error = read(
            &path,
            "link.png",
            ChangeStatus::Untracked,
            ImageSide::Modified,
        );

        assert!(
            matches!(error, Err(FileImageError::Outside(_))),
            "{error:?}"
        );
        assert_eq!(
            error.unwrap_err().to_string(),
            "link.png points outside the workspace, so it is not shown"
        );
    }

    #[test]
    fn refuses_a_file_that_is_not_an_image() {
        let (_root, path) = prepared();
        std::fs::write(path.join("a.ts"), "export {}\n").unwrap();

        let error = read(&path, "a.ts", ChangeStatus::Untracked, ImageSide::Modified);

        assert_eq!(error, Err(FileImageError::NotAnImage("a.ts".to_string())));
        assert_eq!(
            error.unwrap_err().to_string(),
            "a.ts is not an image that the dashboard can show"
        );
    }
}
