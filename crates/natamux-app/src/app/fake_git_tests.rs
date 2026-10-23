use futures::channel::mpsc::UnboundedReceiver;

use super::*;

const REVIEWED: &str = "T-0143";
const IMAGE: &str = "T-0147";

fn paths(fake: &FakeDaemon, id: &str) -> Vec<String> {
    let data = fake.data();
    data.changed(id)
        .iter()
        .map(|entry| entry.path.clone())
        .collect()
}

fn heard(receiver: &mut UnboundedReceiver<DashboardEvent>) -> Vec<DashboardEvent> {
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    events
}

fn code(error: ClientError) -> (u16, String) {
    match error {
        ClientError::Status { status, message } => (status, message),
        other => panic!("not a status error: {other}"),
    }
}

#[test]
fn the_seed_reads() {
    let fake = FakeDaemon::default();
    assert_eq!(
        paths(&fake, REVIEWED),
        ["src/checkout.ts", "test/coupon.test.ts"]
    );
    assert_eq!(paths(&fake, IMAGE), ["public/banner.png"]);
    assert!(paths(&fake, "T-0144").is_empty());
    let data = fake.data();
    let graph = &data.graphs[REVIEWED];
    assert_eq!(graph.commits.len(), 10);
    assert_eq!(
        graph
            .commits
            .iter()
            .filter(|commit| commit.on_branch)
            .count(),
        6
    );
    assert_eq!(graph.commits[0].hash, graph.head);
    assert_eq!(
        graph.commits[0].refs,
        ["natamux/t-0143-rust-migration-phase-1"]
    );
    assert!(graph.commits.iter().any(|commit| commit.refs.len() == 3));
    for commit in &graph.commits {
        assert!(commit.hash.len() >= 7 && commit.hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(commit.date.as_str() < NOW);
    }
}

#[test]
fn a_file_diff_has_both_texts_and_two_hunks_worth_of_lines() {
    let fake = FakeDaemon::default();
    let edited = fake.file_diff(REVIEWED, "src/checkout.ts").unwrap();
    assert_eq!(edited.status, ChangeStatus::Modified);
    let original = edited.original.unwrap();
    let modified = edited.modified.unwrap();
    assert!(original.contains("return total(items) - coupon\n"));
    assert!(modified.contains("const amount = total(items) - Math.max(0, coupon)\n"));
    assert!(modified.contains("return Math.max(0, amount)\n"));
    assert_eq!(modified.lines().count(), original.lines().count() + 2);
    let new = fake.file_diff(REVIEWED, "test/coupon.test.ts").unwrap();
    assert_eq!(new.status, ChangeStatus::Untracked);
    assert_eq!(new.original, None);
    assert_eq!(new.modified.unwrap().lines().count(), 5);
}

#[test]
fn a_diff_of_a_path_that_is_not_changed_is_404() {
    let fake = FakeDaemon::default();
    let missing = fake.file_diff(REVIEWED, "src/none.ts").unwrap_err();
    assert_eq!(code(missing).0, 404);
    assert_eq!(
        code(fake.file_diff("T-0144", "src/checkout.ts").unwrap_err()).0,
        404
    );
}

#[test]
fn an_image_is_a_480_by_300_png_and_other_cases_are_refused() {
    let fake = FakeDaemon::default();
    let bytes = fake
        .file_image(IMAGE, "public/banner.png", ImageSide::Modified)
        .unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&bytes[12..16], b"IHDR");
    assert_eq!(
        u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        480
    );
    assert_eq!(
        u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
        300
    );
    let not_image = fake.file_image(REVIEWED, "src/checkout.ts", ImageSide::Modified);
    assert_eq!(code(not_image.unwrap_err()).0, 400);
    let new_file = fake.file_image(IMAGE, "public/banner.png", ImageSide::Original);
    assert_eq!(code(new_file.unwrap_err()).0, 404);
    let unchanged = fake.file_image(IMAGE, "public/other.png", ImageSide::Modified);
    assert_eq!(code(unchanged.unwrap_err()).0, 404);
}

#[test]
fn a_commit_clears_the_changes_moves_the_branch_and_tells_the_feed() {
    let fake = FakeDaemon::default();
    let (mut feed, _sender) = fake.watch();
    heard(&mut feed);
    let before = fake.data().graphs[REVIEWED].clone();
    let done = fake
        .commit(REVIEWED, "Fix the total\n\nBody".into())
        .unwrap();
    assert!(paths(&fake, REVIEWED).is_empty());
    let data = fake.data();
    let graph = &data.graphs[REVIEWED];
    assert_eq!(graph.head, done.hash);
    assert_eq!(graph.commits.len(), before.commits.len() + 1);
    let top = &graph.commits[0];
    assert_eq!(top.parents, std::slice::from_ref(&before.head));
    assert_eq!(top.subject, "Fix the total");
    assert!(top.on_branch);
    assert_eq!(top.refs, before.commits[0].refs);
    assert!(graph.commits[1].refs.is_empty());
    assert_eq!(graph.ahead, Some(7));
    assert_eq!(
        graph.push,
        Some(PushStatus::Tracked {
            unpushed: 3,
            unpulled: 0
        })
    );
    assert_eq!(done.task.updated_at, NOW);
    assert!(
        data.events[REVIEWED]
            .iter()
            .any(|stored| stored.event.kind == "git.committed")
    );
    drop(data);
    let events = heard(&mut feed);
    assert!(events.iter().any(|event| matches!(
        event,
        DashboardEvent::WorkspaceChanged { task_id } if task_id == REVIEWED
    )));
}

#[test]
fn a_commit_with_nothing_changed_is_409() {
    let fake = FakeDaemon::default();
    let (status, message) = code(fake.commit("T-0144", "x".into()).unwrap_err());
    assert_eq!(status, 409);
    assert_eq!(
        message,
        "There is nothing to commit. The workspace matches its last commit."
    );
    assert_eq!(code(fake.commit_message("T-0144").unwrap_err()).0, 409);
    assert_eq!(code(fake.commit("T-9999", "x".into()).unwrap_err()).0, 404);
}

#[test]
fn a_drafted_message_names_the_title_and_the_files() {
    let fake = FakeDaemon::default();
    let draft = fake.commit_message(REVIEWED).unwrap();
    assert_eq!(draft.source, DraftSource::Agent);
    assert_eq!(draft.agent, "claude-code");
    assert!(draft.message.contains("- src/checkout.ts"));
    assert!(draft.message.contains("- test/coupon.test.ts"));
    assert!(draft.message.lines().next().unwrap().chars().count() <= COMMIT_SUBJECT_MAX);
}

#[test]
fn a_rollback_can_be_undone() {
    let fake = FakeDaemon::default();
    let (mut feed, _sender) = fake.watch();
    heard(&mut feed);
    let done = fake
        .rollback(REVIEWED, vec!["test/coupon.test.ts".into()])
        .unwrap();
    assert_eq!(done.files, 1);
    assert_eq!(paths(&fake, REVIEWED), ["src/checkout.ts"]);
    let events = heard(&mut feed);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DashboardEvent::WorkspaceChanged { .. }))
    );
    let back = fake.undo_rollback(REVIEWED).unwrap();
    assert_eq!(back.files, 1);
    assert_eq!(
        paths(&fake, REVIEWED),
        ["src/checkout.ts", "test/coupon.test.ts"]
    );
    assert_eq!(
        code(fake.undo_rollback(REVIEWED).unwrap_err()),
        (409, "Nothing to undo.".into())
    );
}

#[test]
fn a_rollback_refuses_a_missing_path_and_a_conflict_and_changes_nothing() {
    let fake = FakeDaemon::default();
    let both = vec!["src/checkout.ts".into(), "src/none.ts".into()];
    assert_eq!(code(fake.rollback(REVIEWED, both).unwrap_err()).0, 404);
    assert_eq!(paths(&fake, REVIEWED).len(), 2);
    fake.data().changes.get_mut(REVIEWED).unwrap()[0].status = ChangeStatus::Conflict;
    let (status, message) = code(
        fake.rollback(REVIEWED, vec!["src/checkout.ts".into()])
            .unwrap_err(),
    );
    assert_eq!(status, 409);
    assert_eq!(message, "src/checkout.ts has a conflict. Resolve it first");
    assert_eq!(paths(&fake, REVIEWED).len(), 2);
}

#[test]
fn a_commit_drops_the_undo() {
    let fake = FakeDaemon::default();
    fake.rollback(REVIEWED, vec!["test/coupon.test.ts".into()])
        .unwrap();
    fake.commit(REVIEWED, "Keep one file".into()).unwrap();
    assert_eq!(code(fake.undo_rollback(REVIEWED).unwrap_err()).0, 409);
    assert!(paths(&fake, REVIEWED).is_empty());
}

#[test]
fn a_comment_needs_an_agent_in_terminal_1() {
    let fake = FakeDaemon::default();
    let task = fake.send_to_agent(REVIEWED, "Add a test".into()).unwrap();
    assert_eq!(task.updated_at, NOW);
    let events = fake.list_events(REVIEWED).unwrap();
    let last = events.last().unwrap();
    assert_eq!(last.event.kind, "review.commented");
    assert_eq!(last.event.payload["text"], "Add a test");
    let (status, message) = code(fake.send_to_agent("T-0139", "Hi".into()).unwrap_err());
    assert_eq!(status, 409);
    assert_eq!(
        message,
        "No agent runs in the terminal of T-0139. Start the agent first"
    );
    assert_eq!(
        code(fake.send_to_agent("T-9999", "Hi".into()).unwrap_err()).0,
        404
    );
}
