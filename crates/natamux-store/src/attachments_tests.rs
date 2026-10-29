use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "natamux-attachments-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn file(name: &str, text: &str) -> AttachmentFile {
    AttachmentFile {
        name: name.into(),
        bytes: text.as_bytes().to_vec(),
    }
}

fn attachment(name: &str, size: u64, kind: Option<&str>) -> Attachment {
    Attachment {
        name: name.into(),
        size,
        kind: kind.map(str::to_string),
    }
}

fn names(data_dir: &Path) -> Vec<String> {
    list_attachments(data_dir, "T-0001")
        .unwrap()
        .into_iter()
        .map(|each| each.name)
        .collect()
}

#[test]
fn keeps_the_files_in_the_task_folder_of_the_data_folder_and_lists_them_by_name() {
    let data = TempDir::new();
    save_attachments(
        &data.0,
        "T-0001",
        &[file("b.pdf", "pdf"), file("a.png", "image")],
    )
    .unwrap();

    let saved = data.0.join("tasks/T-0001/attachments/a.png");
    assert_eq!(fs::read_to_string(saved).unwrap(), "image");
    assert_eq!(
        list_attachments(&data.0, "T-0001").unwrap(),
        [
            attachment("a.png", 5, Some("image/png")),
            attachment("b.pdf", 3, None)
        ]
    );
}

#[test]
fn lists_nothing_for_a_task_without_attachments_and_writes_no_folder_for_none() {
    let data = TempDir::new();
    save_attachments(&data.0, "T-0001", &[]).unwrap();
    assert!(!attachments_dir(&data.0, "T-0001").exists());
    assert!(list_attachments(&data.0, "T-0001").unwrap().is_empty());
}

#[test]
fn leaves_out_folders_and_files_with_names_that_are_not_safe() {
    let data = TempDir::new();
    let dir = attachments_dir(&data.0, "T-0001");
    fs::create_dir_all(dir.join("inner")).unwrap();
    fs::write(dir.join(".hidden"), "x").unwrap();
    fs::write(dir.join("ok.txt"), "x").unwrap();
    assert_eq!(names(&data.0), ["ok.txt"]);
}

#[test]
fn gives_the_bytes_of_an_attachment_and_none_for_a_name_the_task_does_not_have() {
    let data = TempDir::new();
    save_attachments(&data.0, "T-0001", &[file("a.txt", "hello")]).unwrap();
    assert_eq!(
        read_attachment(&data.0, "T-0001", "a.txt")
            .unwrap()
            .unwrap(),
        Some(b"hello".to_vec())
    );
    assert_eq!(
        read_attachment(&data.0, "T-0001", "b.txt")
            .unwrap()
            .unwrap(),
        None
    );
}

#[test]
fn refuses_a_name_that_could_leave_the_attachments_folder() {
    let data = TempDir::new();
    for name in ["../T-0001.md", "..", ".hidden", "a b.txt"] {
        assert_eq!(
            read_attachment(&data.0, "T-0001", name).unwrap_err(),
            UnsafeName(name.into())
        );
    }
}

#[test]
fn reads_every_attachment_for_a_copy_of_the_task() {
    let data = TempDir::new();
    save_attachments(&data.0, "T-0001", &[file("a.txt", "A"), file("b.txt", "B")]).unwrap();
    assert_eq!(
        read_attachments(&data.0, "T-0001").unwrap(),
        [file("a.txt", "A"), file("b.txt", "B")]
    );
}

#[test]
fn copies_the_attachments_into_the_stage_folder_of_the_worktree() {
    let data = TempDir::new();
    let worktree = TempDir::new();
    save_attachments(&data.0, "T-0001", &[file("a.png", "image")]).unwrap();
    restore_attachments(&data.0, "T-0001", Some(&worktree.0)).unwrap();
    let copy = worktree.0.join(".natamux-task/attachments/a.png");
    assert_eq!(fs::read_to_string(copy).unwrap(), "image");
    // The archive keeps its copy, so a removed workspace loses none.
    assert_eq!(names(&data.0), ["a.png"]);
}

#[test]
fn does_nothing_for_a_task_without_attachments_or_without_a_worktree() {
    let data = TempDir::new();
    let worktree = TempDir::new();
    restore_attachments(&data.0, "T-0001", Some(&worktree.0)).unwrap();
    assert!(!worktree.0.join(".natamux-task").exists());
    save_attachments(&data.0, "T-0001", &[file("a.png", "image")]).unwrap();
    restore_attachments(&data.0, "T-0001", None).unwrap();
}

fn listed() -> Vec<Attachment> {
    vec![
        attachment("login-error.png", 12_300, Some("image/png")),
        attachment("spec.docx", 512, None),
    ]
}

fn section() -> String {
    [
        "## Attachments",
        "",
        "Files attached to the task, by their path from the root of the worktree:",
        "",
        "- `.natamux-task/attachments/login-error.png` (12.3 KB)",
        "- `.natamux-task/attachments/spec.docx` (512 B)",
        "",
    ]
    .join("\n")
}

#[test]
fn lists_each_file_by_its_path_from_the_worktree_root_with_its_size() {
    assert_eq!(attachment_section(&listed()), section());
}

#[test]
fn adds_the_section_after_a_blank_line_at_the_end_of_the_context() {
    assert_eq!(
        with_attachment_section("# Fix it\n\nNotes.\n", &listed()),
        format!("# Fix it\n\nNotes.\n\n{}", section())
    );
    assert_eq!(
        with_attachment_section("# Fix it", &listed()),
        format!("# Fix it\n\n{}", section())
    );
}

#[test]
fn leaves_a_context_with_its_own_attachments_heading_as_it_is() {
    let text = "# Fix it\n\n### attachments\n\n- the screenshot\n";
    assert_eq!(with_attachment_section(text, &listed()), text);
}

#[test]
fn leaves_the_context_of_a_task_without_attachments_as_it_is() {
    assert_eq!(with_attachment_section("# Fix it\n", &[]), "# Fix it\n");
}

#[cfg(unix)]
#[test]
fn does_not_restore_through_a_link_in_the_worktree() {
    use std::os::unix::fs::symlink;
    let data = TempDir::new();
    let outside = TempDir::new();
    save_attachments(&data.0, "T-0001", &[file("a.png", "image")]).unwrap();
    let outside_count = || fs::read_dir(&outside.0).unwrap().count();

    // A link as the stage folder.
    let first = TempDir::new();
    symlink(&outside.0, first.0.join(".natamux-task")).unwrap();
    let error = restore_attachments(&data.0, "T-0001", Some(&first.0)).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);

    // A link as the attachments folder.
    let second = TempDir::new();
    fs::create_dir(second.0.join(".natamux-task")).unwrap();
    symlink(&outside.0, second.0.join(".natamux-task/attachments")).unwrap();
    assert!(restore_attachments(&data.0, "T-0001", Some(&second.0)).is_err());

    // A dangling link as the file.
    let third = TempDir::new();
    fs::create_dir_all(third.0.join(".natamux-task/attachments")).unwrap();
    symlink(
        outside.0.join("target"),
        third.0.join(".natamux-task/attachments/a.png"),
    )
    .unwrap();
    assert!(restore_attachments(&data.0, "T-0001", Some(&third.0)).is_err());
    assert_eq!(outside_count(), 0);
}
