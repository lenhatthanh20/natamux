use natamux_proto::projects::repo::Repo;

use super::*;

fn task(id: &str, state: TaskState) -> Task {
    Task {
        id: id.into(),
        title: format!("Title of {id}"),
        state,
        ..Task::default()
    }
}

fn ids(tasks: &[&Task]) -> Vec<String> {
    tasks.iter().map(|task| task.id.clone()).collect()
}

#[test]
fn lists_tasks_that_moved_into_a_state_that_needs_the_user() {
    let before = states_by_id(&[
        task("T-0001", TaskState::Running),
        task("T-0002", TaskState::Running),
        task("T-0003", TaskState::Review),
        task("T-0004", TaskState::Running),
    ]);
    let now = [
        task("T-0001", TaskState::NeedsInput),
        task("T-0002", TaskState::Running),
        task("T-0003", TaskState::Failed),
        task("T-0004", TaskState::Review),
        task("T-0005", TaskState::Failed),
    ];
    assert_eq!(
        ids(&tasks_that_started_waiting(&before, &now)),
        ["T-0001", "T-0004"]
    );
}

#[test]
fn lists_nothing_the_first_time() {
    let now = [task("T-0001", TaskState::Review)];
    assert!(tasks_that_started_waiting(&States::new(), &now).is_empty());
}

fn at_stage(id: &str, state: TaskState, stage: &str) -> Task {
    Task {
        stage: stage.into(),
        repo: Some("repo-1".into()),
        ..task(id, state)
    }
}

fn repos() -> Vec<Repo> {
    vec![Repo {
        id: "repo-1".into(),
        name: "natamux".into(),
        path: "/src/natamux".into(),
        worktrees_dir: None,
    }]
}

#[test]
fn titles_the_note_with_the_task_and_says_what_happened_where() {
    assert_eq!(
        notification_for(&at_stage("T-0001", TaskState::Review, "spec"), &repos()),
        Notification {
            title: "Title of T-0001".into(),
            body: "Spec waits for your approval\nT-0001 · natamux".into()
        }
    );
}

fn staged(state: TaskState) -> String {
    let task = at_stage("T-0001", state, "test-spec");
    notification_for(&task, &repos()).body
}

fn bare(state: TaskState) -> String {
    notification_for(&task("T-0002", state), &repos()).body
}

#[test]
fn says_why_each_state_rings_with_its_stage() {
    assert_eq!(
        staged(TaskState::NeedsInput),
        "Needs your input at Test spec\nT-0001 · natamux"
    );
    assert_eq!(
        staged(TaskState::Failed),
        "Failed at Test spec\nT-0001 · natamux"
    );
    assert_eq!(
        staged(TaskState::Blocked),
        "Blocked at Test spec\nT-0001 · natamux"
    );
    assert_eq!(staged(TaskState::Done), "Done\nT-0001 · natamux");
}

#[test]
fn leaves_out_the_stage_and_the_project_it_does_not_know() {
    assert_eq!(bare(TaskState::NeedsInput), "Needs your input\nT-0002");
    assert_eq!(bare(TaskState::Review), "Waits for your review\nT-0002");
    assert_eq!(bare(TaskState::Failed), "Failed\nT-0002");
    assert_eq!(bare(TaskState::Blocked), "Blocked\nT-0002");
    let gone = Task {
        repo: Some("removed".into()),
        ..task("T-0003", TaskState::Done)
    };
    assert_eq!(notification_for(&gone, &repos()).body, "Done\nT-0003");
}

#[test]
fn rings_for_tasks_that_started_waiting_and_then_for_tasks_that_finished() {
    let before = states_by_id(&[
        task("T-0001", TaskState::Running),
        task("T-0002", TaskState::Review),
        task("T-0003", TaskState::Running),
    ]);
    let now = [
        task("T-0001", TaskState::Done),
        task("T-0002", TaskState::Running),
        task("T-0003", TaskState::NeedsInput),
    ];
    let ids: Vec<String> = rings(&before, &now, &[])
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, ["T-0003", "T-0001"]);
}

#[test]
fn keeps_the_note_to_two_lines_when_a_name_has_a_line_break() {
    let mut repos = repos();
    repos[0].name = "nata\nmux\r\nweb".into();
    let task = Task {
        id: "T-00\n01".into(),
        ..at_stage("T-0001", TaskState::Done, "spec")
    };
    assert_eq!(
        notification_for(&task, &repos).body,
        "Done\nT-00 01 · nata mux  web"
    );
}

fn ring_list(count: usize) -> Vec<(String, Notification)> {
    (0..count)
        .map(|n| {
            let done = task(&format!("T-000{n}"), TaskState::Done);
            (done.id.clone(), notification_for(&done, &[]))
        })
        .collect()
}

#[test]
fn plays_one_sound_for_a_batch_of_rings_and_none_for_no_ring() {
    assert!(!plays_sound(&ring_list(0)));
    assert!(plays_sound(&ring_list(1)));
    assert!(plays_sound(&ring_list(3)));
}

#[test]
fn shows_no_dock_badge_when_nothing_waits() {
    assert_eq!(badge_label(0), None);
    assert_eq!(badge_label(1).as_deref(), Some("1"));
    assert_eq!(badge_label(42).as_deref(), Some("42"));
}

#[test]
fn asks_for_permission_with_a_note_that_opens_no_task() {
    let note = turned_on();
    assert_eq!(note.title.as_ref(), "Notifications are on");
    assert!(!note.tag.starts_with("T-"));
    assert!(note.actions.is_empty());
}

#[test]
fn lists_tasks_that_became_done_not_cancelled_ones() {
    let before = states_by_id(&[
        task("T-0001", TaskState::Review),
        task("T-0002", TaskState::Running),
        task("T-0003", TaskState::Done),
    ]);
    let now = [
        task("T-0001", TaskState::Done),
        task("T-0002", TaskState::Cancelled),
        task("T-0003", TaskState::Done),
        task("T-0004", TaskState::Done),
    ];
    assert_eq!(ids(&tasks_that_finished(&before, &now)), ["T-0001"]);
}

fn waiting(id: &str, title: &str) -> Task {
    Task {
        title: title.into(),
        ..task(id, TaskState::NeedsInput)
    }
}

fn first() -> Task {
    waiting("T-0001", "Fix the token refresh race in the login flow")
}

#[test]
fn says_nothing_for_the_tasks_that_wait_when_the_page_opens() {
    let mut announcer = NeedsYouAnnouncer::default();
    announcer.update(&[first()], true);
    assert_eq!(announcer.text(), "");
}

#[test]
fn says_nothing_for_the_tasks_that_wait_when_the_task_list_first_loads() {
    let mut announcer = NeedsYouAnnouncer::default();
    announcer.update(&[], false);
    announcer.update(&[first()], true);
    assert_eq!(announcer.text(), "");
}

#[test]
fn names_a_task_that_starts_to_wait_politely() {
    let mut announcer = NeedsYouAnnouncer::default();
    announcer.update(&[first()], true);
    announcer.update(&[first(), waiting("T-0002", "Add CSV export")], true);
    assert_eq!(announcer.text(), "Add CSV export needs you");
}

#[test]
fn names_every_task_that_starts_to_wait_at_once() {
    let mut announcer = NeedsYouAnnouncer::default();
    announcer.update(&[first()], true);
    announcer.update(
        &[
            first(),
            waiting("T-0002", "Add CSV export"),
            waiting("T-0003", "Fix login"),
        ],
        true,
    );
    assert_eq!(
        announcer.text(),
        "Add CSV export needs you. Fix login needs you"
    );
}

#[test]
fn says_nothing_new_when_a_task_stops_waiting() {
    let mut announcer = NeedsYouAnnouncer::default();
    announcer.update(&[first(), waiting("T-0002", "Add CSV export")], true);
    announcer.update(&[first()], true);
    assert_eq!(announcer.text(), "");
}

#[test]
fn names_a_task_again_when_it_stops_waiting_and_then_waits_again() {
    let mut announcer = NeedsYouAnnouncer::default();
    let later = waiting("T-0002", "Ship docs");
    let other = waiting("T-0003", "Fix login");
    announcer.update(&[first()], true);
    announcer.update(&[first(), later.clone()], true);
    announcer.update(&[first(), other.clone()], true);
    assert_eq!(announcer.text(), "Fix login needs you");
    announcer.update(&[first(), other, later], true);
    assert_eq!(announcer.text(), "Ship docs needs you");
}
