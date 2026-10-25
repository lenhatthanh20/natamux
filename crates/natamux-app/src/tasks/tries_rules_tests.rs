use natamux_proto::tasks::task::TaskState;

use super::*;

fn task(id: &str, labels: &[&str], state: TaskState) -> Task {
    Task {
        id: id.into(),
        title: id.into(),
        state,
        labels: labels.iter().map(|label| label.to_string()).collect(),
        ..Task::default()
    }
}

fn ids(tasks: &[&Task]) -> Vec<String> {
    tasks.iter().map(|task| task.id.clone()).collect()
}

const GROUP: &str = "tries-t-0007";

fn all() -> Vec<Task> {
    vec![
        task("T-0010", &[GROUP], TaskState::Cancelled),
        task("T-0007", &["perf", GROUP], TaskState::Running),
        task("T-0008", &[GROUP], TaskState::Review),
        task("T-0009", &["other"], TaskState::Running),
    ]
}

#[test]
fn finds_the_group_of_a_task() {
    let all = all();
    assert_eq!(try_group(&all[1]), Some(GROUP));
    assert_eq!(try_group(&all[3]), None);
}

#[test]
fn lists_the_tries_of_a_group_in_order() {
    let all = all();
    assert_eq!(ids(&tries_of(GROUP, &all)), ["T-0007", "T-0008", "T-0010"]);
}

#[test]
fn cancels_the_other_tries_that_are_not_finished() {
    let all = all();
    let tries = tries_of(GROUP, &all);
    assert_eq!(ids(&tries_to_cancel(tries[1], &tries)), ["T-0007"]);
}

#[test]
fn reads_each_label_as_a_group_or_not() {
    let table = [
        ("tries-t-0005", true),
        ("tries-task-0005", true),
        ("tries-a1-2", false),
        ("tries-task", false),
        ("tries-TASK-0005", false),
    ];
    for (label, is_group) in table {
        let one = task("T-1", &[label], TaskState::Ready);
        assert_eq!(try_group(&one), is_group.then_some(label), "{label}");
    }
}

#[test]
fn groups_the_tries_of_a_task_with_its_own_id_and_lists_that_task_first() {
    let custom = "tries-task-0500";
    let tries = [
        task("T-0012", &[custom], TaskState::Running),
        task("TASK-0500", &[custom], TaskState::Running),
        task("T-0011", &[custom], TaskState::Running),
    ];
    assert_eq!(try_group(&tries[1]), Some(custom));
    assert_eq!(
        ids(&tries_of(custom, &tries)),
        ["TASK-0500", "T-0011", "T-0012"]
    );
}

#[test]
fn writes_the_diff_stat_of_a_try() {
    let stat = DiffStat {
        files: 2,
        added: 12,
        removed: 3,
    };
    assert_eq!(diff_stat_text(Some(&stat)), "+12 −3, 2 files");
    assert_eq!(diff_stat_text(Some(&DiffStat::default())), "-");
    assert_eq!(diff_stat_text(None), "-");
}

#[test]
fn offers_keep_this_try_only_when_it_is_not_done_or_cancelled() {
    let all = all();
    assert!(can_keep(&all[1]));
    assert!(!can_keep(&all[0]));
    assert!(!can_keep(&task("T-1", &[], TaskState::Done)));
}
