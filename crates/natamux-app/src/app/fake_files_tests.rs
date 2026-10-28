use futures::channel::mpsc::UnboundedReceiver;

use super::*;

const TASK: &str = "T-0143";

fn code(error: ClientError) -> (u16, String) {
    match error {
        ClientError::Status { status, message } => (status, message),
        other => panic!("not a status error: {other}"),
    }
}

fn names(entries: &[TreeEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

fn watch(fake: &FakeDaemon) -> UnboundedReceiver<DashboardEvent> {
    let (mut receiver, _feed) = fake.watch();
    while receiver.try_recv().is_ok() {}
    receiver
}

fn changed(receiver: &mut UnboundedReceiver<DashboardEvent>) -> bool {
    let mut seen = false;
    while let Ok(event) = receiver.try_recv() {
        seen |= matches!(event, DashboardEvent::WorkspaceChanged { .. });
    }
    seen
}

#[test]
fn the_root_lists_folders_first_then_files_by_name() {
    let root = FakeDaemon::default().list_folder(TASK, "").unwrap();
    assert_eq!(
        names(&root),
        [
            ".claude",
            ".codex",
            ".github",
            ".natamux-task",
            "data",
            "public",
            "src",
            "test",
            ".gitignore",
            ".natamux.json",
            "package.json",
            "README.md",
        ]
    );
    assert!(
        root[..8]
            .iter()
            .all(|entry| entry.kind == EntryKind::Folder)
    );
    assert!(root[8..].iter().all(|entry| entry.kind == EntryKind::File));
}

#[test]
fn a_folder_lists_its_own_rows() {
    let fake = FakeDaemon::default();
    let src = fake.list_folder(TASK, "src").unwrap();
    assert_eq!(names(&src), ["cart.ts", "checkout.ts", "format.ts"]);
    let github = fake.list_folder(TASK, ".github").unwrap();
    assert_eq!(names(&github), ["workflows", "README.md"]);
    let docs = fake.list_folder(TASK, ".natamux-task").unwrap();
    assert_eq!(docs.len(), 6);
    let small = fake.list_folder("T-0146", "").unwrap();
    assert_eq!(names(&small), ["src", "README.md"]);
}

#[test]
fn numbers_in_names_sort_in_order() {
    assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
    assert_eq!(natural_cmp("a10", "a2"), Ordering::Greater);
    assert_eq!(natural_cmp("b", "B"), Ordering::Equal);
    assert_eq!(natural_cmp("a", "ab"), Ordering::Less);
}

#[test]
fn reads_text_binary_and_too_large_files() {
    let fake = FakeDaemon::default();
    let readme = fake.read_workspace_file(TASK, "README.md").unwrap();
    assert_eq!(
        readme.text.as_deref(),
        Some("# Storefront\n\nA small shop: catalog, cart and checkout.\n")
    );
    assert_eq!(readme.version, "v1-README.md");
    let checkout = fake.read_workspace_file(TASK, "src/checkout.ts").unwrap();
    assert!(checkout.text.unwrap().contains("Math.max(0, amount)"));
    let logo = fake.read_workspace_file(TASK, "public/logo.png").unwrap();
    assert_eq!(
        (logo.text, logo.binary, logo.too_large),
        (None, true, false)
    );
    let big = fake.read_workspace_file(TASK, "data/catalog.json").unwrap();
    assert_eq!((big.text, big.binary, big.too_large), (None, false, true));
    assert_eq!(big.version, "");
}

#[test]
fn a_save_stores_the_text_and_moves_the_version() {
    let fake = FakeDaemon::default();
    let mut feed = watch(&fake);
    let saved = fake
        .write_workspace_file(TASK, "README.md", "new\n".into(), "v1-README.md".into())
        .unwrap();
    assert_eq!(saved.version, "v2-README.md");
    assert!(changed(&mut feed));
    let read = fake.read_workspace_file(TASK, "README.md").unwrap();
    assert_eq!(read.text.as_deref(), Some("new\n"));
    assert_eq!(read.version, saved.version);
}

#[test]
fn a_save_with_an_old_version_is_a_conflict_and_changes_nothing() {
    let fake = FakeDaemon::default();
    fake.write_workspace_file(TASK, "README.md", "new\n".into(), "v1-README.md".into())
        .unwrap();
    let mut feed = watch(&fake);
    let (status, message) = code(
        fake.write_workspace_file(TASK, "README.md", "late\n".into(), "v1-README.md".into())
            .unwrap_err(),
    );
    assert_eq!(status, 409);
    assert!(message.starts_with("README.md changed on disk"));
    assert!(!changed(&mut feed));
    let read = fake.read_workspace_file(TASK, "README.md").unwrap();
    assert_eq!(read.text.as_deref(), Some("new\n"));
}

#[test]
fn a_file_the_agent_wrote_makes_the_version_old() {
    let fake = FakeDaemon::default();
    let mut feed = watch(&fake);
    fake.set_changed_on_disk(TASK, "src/checkout.ts");
    assert!(changed(&mut feed));
    let answer = fake.write_workspace_file(
        TASK,
        "src/checkout.ts",
        "x".into(),
        "v1-src/checkout.ts".into(),
    );
    assert_eq!(code(answer.unwrap_err()).0, 409);
}

#[test]
fn a_too_large_file_is_not_saved() {
    let fake = FakeDaemon::default();
    let answer = fake.write_workspace_file(TASK, "data/catalog.json", "x".into(), "".into());
    assert_eq!(code(answer.unwrap_err()).0, 413);
}

#[test]
fn bad_paths_and_unknown_tasks_fail_like_the_daemon() {
    let fake = FakeDaemon::default();
    let (status, message) = code(fake.read_workspace_file(TASK, "../x").unwrap_err());
    assert_eq!(status, 409);
    assert_eq!(
        message,
        "../x points outside the workspace, so it is not shown"
    );
    assert_eq!(
        code(fake.list_folder(TASK, "src/../..").unwrap_err()).0,
        409
    );
    let (status, message) = code(fake.read_workspace_file(TASK, "gone.ts").unwrap_err());
    assert_eq!(
        (status, message.as_str()),
        (404, "gone.ts is not in the workspace")
    );
    assert_eq!(code(fake.list_folder(TASK, "gone").unwrap_err()).0, 404);
    assert_eq!(
        code(fake.list_folder(TASK, "README.md").unwrap_err()).0,
        400
    );
    assert_eq!(code(fake.list_folder("T-9999", "").unwrap_err()).0, 404);
    assert_eq!(code(fake.list_folder("T-0148", "").unwrap_err()).0, 409);
}

#[test]
fn the_stage_files_have_texts_that_match_their_list() {
    let fake = FakeDaemon::default();
    let listed = fake.list_stage_files(TASK).unwrap();
    assert_eq!(listed.len(), 6);
    for file in &listed {
        let text = fake.read_stage_file(TASK, &file.name).unwrap().text;
        assert_eq!(file.size, text.len() as u64, "{}", file.name);
        assert!(file.modified_at.as_str() < NOW);
    }
    let context = fake.read_stage_file(TASK, "context.md").unwrap();
    assert!(context.text.starts_with(
        "# Fix the bug where a big coupon makes the total negative\n\nA coupon larger than"
    ));
    let spec = fake.read_stage_file(TASK, "spec.md").unwrap().text;
    for part in ["\n## ", "\n- ", "|---|", "```ts", "](https://"] {
        assert!(spec.contains(part), "{part}");
    }
    let (status, message) = code(fake.read_stage_file(TASK, "none.md").unwrap_err());
    assert_eq!((status, message.as_str()), (404, "T-0143 has no none.md"));
}

#[test]
fn writing_a_stage_file_stores_it_and_moves_its_time() {
    let fake = FakeDaemon::default();
    let mut feed = watch(&fake);
    fake.write_stage_file(TASK, "spec.md", "# Short\n".into())
        .unwrap();
    assert!(changed(&mut feed));
    assert_eq!(
        fake.read_stage_file(TASK, "spec.md").unwrap().text,
        "# Short\n"
    );
    let listed = fake.list_stage_files(TASK).unwrap();
    let spec = listed.iter().find(|file| file.name == "spec.md").unwrap();
    assert_eq!((spec.size, spec.modified_at.as_str()), (8, NOW));
    fake.write_stage_file(TASK, "extra.md", "x".into()).unwrap();
    assert_eq!(fake.list_stage_files(TASK).unwrap().len(), 7);
}

#[test]
fn the_attachments_list_and_read() {
    let fake = FakeDaemon::default();
    let list = fake.list_attachments(TASK).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "screen.png");
    assert_eq!(list[0].kind.as_deref(), Some("image/png"));
    let bytes = fake.read_attachment(TASK, "screen.png").unwrap();
    assert_eq!(list[0].size, bytes.len() as u64);
    assert_eq!(&bytes[1..4], b"PNG");
    assert_eq!(list[1].kind, None);
    assert!(
        fake.read_attachment(TASK, "notes.pdf")
            .unwrap()
            .starts_with(b"%PDF")
    );
    assert_eq!(
        code(fake.read_attachment(TASK, "x.png").unwrap_err()).0,
        404
    );
    assert!(fake.list_attachments("T-0141").unwrap().is_empty());
}
