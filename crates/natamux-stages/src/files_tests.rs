use super::*;
use natamux_git::test_support::{TempDir, run_git, test_repo};

#[test]
fn accepts_markdown_names_only() {
    for name in ["spec.md", "test-spec.md", "a.b.md", "A_1.md"] {
        assert!(check_file_name(name).is_ok(), "{name}");
    }
    for name in [
        ".md",
        "spec.txt",
        "a..md",
        "../x.md",
        "a/b.md",
        "a b.md",
        "spec.md\n",
        "",
    ] {
        assert!(check_file_name(name).is_err(), "{name:?}");
    }
    assert_eq!(
        check_file_name("x.txt").unwrap_err().0,
        "\"x.txt\" is not a stage file. Stage files are markdown, like spec.md"
    );
}

#[test]
fn writes_reads_and_lists_the_files_of_the_worktree() {
    let data = TempDir::new("data");
    let work = TempDir::new("work");
    let workspace = work.path().to_str().unwrap();
    assert!(list_stage_files(data.path(), "T-1", Some(workspace)).is_empty());
    assert_eq!(
        read_stage_file(data.path(), "T-1", Some(workspace), "spec.md"),
        Ok(None)
    );
    write_stage_file(data.path(), "T-1", Some(workspace), "spec.md", "# Spec\n").unwrap();
    write_stage_file(data.path(), "T-1", Some(workspace), "plan.md", "p").unwrap();
    assert_eq!(
        read_stage_file(data.path(), "T-1", Some(workspace), "spec.md"),
        Ok(Some("# Spec\n".to_string()))
    );
    let files = list_stage_files(data.path(), "T-1", Some(workspace));
    let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
    assert_eq!(names, ["plan.md", "spec.md"]);
    assert_eq!(files[1].size, 7);
    assert!(files[1].modified_at.ends_with('Z'));
    assert!(!work.path().join(".natamux-task/spec.md.tmp").exists());
}

#[test]
fn refuses_a_file_over_1_mb_and_a_bad_name() {
    let data = TempDir::new("data");
    let big = "x".repeat(MAX_STAGE_FILE_BYTES + 1);
    assert_eq!(
        write_stage_file(data.path(), "T-1", None, "a.md", &big)
            .unwrap_err()
            .0,
        "a.md is larger than 1 MB"
    );
    assert!(write_stage_file(data.path(), "T-1", None, "a.txt", "x").is_err());
    assert!(read_stage_file(data.path(), "T-1", None, "../a.md").is_err());
}

#[test]
fn archives_and_restores_the_stage_files() {
    let data = TempDir::new("data");
    let work = TempDir::new("work");
    let workspace = work.path().to_str().unwrap();
    write_stage_file(data.path(), "T-1", Some(workspace), "spec.md", "s").unwrap();
    std::fs::write(work.path().join(".natamux-task/notes.txt"), "no").unwrap();
    archive_stage_files(data.path(), "T-1", Some(workspace)).unwrap();
    assert_eq!(
        read_stage_file(data.path(), "T-1", None, "spec.md"),
        Ok(Some("s".to_string()))
    );
    assert!(!archive_dir(data.path(), "T-1").join("notes.txt").exists());

    let fresh = TempDir::new("fresh");
    let fresh_workspace = fresh.path().to_str().unwrap();
    restore_stage_files(data.path(), "T-1", Some(fresh_workspace)).unwrap();
    assert_eq!(
        read_stage_file(data.path(), "T-1", Some(fresh_workspace), "spec.md"),
        Ok(Some("s".to_string()))
    );
    // No workspace and no archive: nothing to do.
    assert!(archive_stage_files(data.path(), "T-1", None).is_ok());
    assert!(restore_stage_files(data.path(), "T-9", Some(fresh_workspace)).is_ok());
}

#[test]
fn adds_the_stage_folder_to_the_ignore_list_once() {
    let (_root, repo) = test_repo();
    exclude_stage_dir(&repo).unwrap();
    exclude_stage_dir(&repo).unwrap();
    let text = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert_eq!(text.matches("/.natamux-task/\n").count(), 1);
    run_git(&repo, &["status"]);
    let dir = TempDir::new("nogit");
    assert!(exclude_stage_dir(dir.path()).is_err());
}

#[test]
fn starts_a_new_line_when_the_ignore_list_has_no_line_end() {
    let (_root, repo) = test_repo();
    std::fs::create_dir_all(repo.join(".git/info")).unwrap();
    std::fs::write(repo.join(".git/info/exclude"), "*.log").unwrap();
    exclude_stage_dir(&repo).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap(),
        "*.log\n/.natamux-task/\n"
    );
}

#[cfg(unix)]
mod links {
    use super::*;
    use std::os::unix::fs::symlink;

    fn outside_is_empty(outside: &TempDir) -> bool {
        std::fs::read_dir(outside.path()).unwrap().next().is_none()
    }

    #[test]
    fn write_stage_file_refuses_a_link_as_the_folder_the_file_or_a_dangling_one() {
        let data = TempDir::new("data");
        let outside = TempDir::new("outside");
        let folder_link = TempDir::new("work");
        symlink(outside.path(), folder_link.path().join(".natamux-task")).unwrap();
        let workspace = folder_link.path().to_str().unwrap();
        let error = write_stage_file(data.path(), "T-1", Some(workspace), "spec.md", "x");
        assert!(error.unwrap_err().0.contains(".natamux-task"));

        let file_link = TempDir::new("work");
        std::fs::create_dir(file_link.path().join(".natamux-task")).unwrap();
        std::fs::write(outside.path().join("secret"), "keep").unwrap();
        symlink(
            outside.path().join("secret"),
            file_link.path().join(".natamux-task/spec.md"),
        )
        .unwrap();
        let workspace = file_link.path().to_str().unwrap();
        assert!(write_stage_file(data.path(), "T-1", Some(workspace), "spec.md", "x").is_err());
        assert_eq!(
            std::fs::read_to_string(outside.path().join("secret")).unwrap(),
            "keep"
        );

        let dangling = TempDir::new("work");
        std::fs::create_dir(dangling.path().join(".natamux-task")).unwrap();
        symlink(
            outside.path().join("new"),
            dangling.path().join(".natamux-task/plan.md"),
        )
        .unwrap();
        let workspace = dangling.path().to_str().unwrap();
        assert!(write_stage_file(data.path(), "T-1", Some(workspace), "plan.md", "x").is_err());
        assert!(!outside.path().join("new").exists());
    }

    #[test]
    fn copy_and_restore_refuse_a_link_in_the_worktree() {
        let data = TempDir::new("data");
        let outside = TempDir::new("outside");
        let source = TempDir::new("source");
        let source_workspace = source.path().to_str().unwrap();
        write_stage_file(data.path(), "T-1", Some(source_workspace), "spec.md", "s").unwrap();
        archive_stage_files(data.path(), "T-1", Some(source_workspace)).unwrap();

        let work = TempDir::new("work");
        symlink(outside.path(), work.path().join(".natamux-task")).unwrap();
        let workspace = work.path().to_str().unwrap();
        assert!(restore_stage_files(data.path(), "T-1", Some(workspace)).is_err());

        // Copy out of a worktree whose archive folder is a link is not a risk (the archive is
        // ours), but a link as the target file in a worktree is.
        let target = TempDir::new("target");
        std::fs::create_dir(target.path().join(".natamux-task")).unwrap();
        symlink(
            outside.path().join("spec.md"),
            target.path().join(".natamux-task/spec.md"),
        )
        .unwrap();
        let from = archive_dir(data.path(), "T-1");
        let copied = copy_stage_files(&from, target.path(), Path::new(STAGE_DIR));
        assert!(copied.is_err());
        assert!(outside_is_empty(&outside));
    }
}
