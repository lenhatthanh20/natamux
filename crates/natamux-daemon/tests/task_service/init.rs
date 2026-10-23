//! The group 'init'.

use super::*;

#[test]
fn rebuilds_the_index_from_the_task_files() {
    let fx = Fixture::new();
    create(&fx.service, "First");
    create(&fx.service, "Second");

    let fresh = build(fx.data(), fx.memory_index(), &fx.messages, Build::default());
    assert_eq!(fresh.list_tasks(None).unwrap(), []);

    fresh.init().unwrap();
    fresh.close();

    let mut found = titles(&fresh.list_tasks(None).unwrap());
    found.sort();
    assert_eq!(found, ["First", "Second"]);
}

#[test]
fn rebuilds_the_index_with_the_tasks_whose_id_the_user_chose() {
    let fx = Fixture::new();
    create(&fx.service, "Task-0005: Fix bug xxx");

    let fresh = build(fx.data(), fx.memory_index(), &fx.messages, Build::default());
    fresh.init().unwrap();
    fresh.close();

    let tasks = fresh.list_tasks(None).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        (tasks[0].id.as_str(), tasks[0].title.as_str()),
        ("TASK-0005", "Fix bug xxx")
    );
}
