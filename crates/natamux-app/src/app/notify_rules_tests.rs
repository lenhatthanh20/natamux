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

#[test]
fn names_the_task_and_its_state() {
    assert_eq!(
        notification_for(&task("T-0001", TaskState::NeedsInput)),
        Notification {
            title: "T-0001: Needs input".into(),
            body: "Title of T-0001".into()
        }
    );
    assert_eq!(
        notification_for(&task("T-0002", TaskState::Review)).title,
        "T-0002: In review"
    );
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
