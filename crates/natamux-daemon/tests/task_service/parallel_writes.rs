//! Changes of tasks from many threads at once: a hook's change writes its files after the store
//! lock, and must still end as a change written under it.

use std::sync::Barrier;
use std::time::Duration;

use natamux_daemon::tasks::errors::TaskError;
use natamux_store::event_log::{event_log_path, read_event_log};
use natamux_store::task_files::{serialize_task, task_file_path};

use super::*;

const THREADS: usize = 8;
const TASKS: usize = 4;
const ROUNDS: usize = 12;

/// The task in memory, in its file and in the index are one, and its log holds the events the
/// index holds, in the same order.
fn assert_whole(fx: &Fixture, id: &str, events: usize) {
    let memory = fx.service.get_task(id).unwrap();
    assert_eq!(file_task(fx, id), Some(memory.clone()));
    let indexed = index_tasks(&fx.service)
        .into_iter()
        .find(|task| task.id == id);
    assert_eq!(indexed, Some(memory));
    let stored = event_pairs(&fx.service.list_events(id).unwrap());
    assert_eq!(stored.len(), events);
    let logged: Vec<(TaskEvent, bool)> = read_event_log(fx.data(), id)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|logged| (logged.event, logged.accepted))
        .collect();
    assert_eq!(logged, stored);
}

fn overridden(to: &str) -> Value {
    json!({ "source": "human", "type": "state.overridden", "payload": { "to": to } })
}

/// A hook-like change, then a check that its event can be read once the call returns.
fn override_and_read(service: &TaskService, id: &str, to: &str) {
    let applied = service.apply_event(id, Some(&overridden(to))).unwrap();
    let events = service.list_events(id).unwrap();
    assert!(
        events
            .iter()
            .any(|stored| stored.event.at == applied.task.updated_at)
    );
}

#[test]
fn many_threads_writing_many_tasks_end_with_memory_file_and_index_alike() {
    let fx = Fixture::new();
    let tasks: Vec<String> = (0..TASKS)
        .map(|number| create(&fx.service, &format!("Task {number}")).id)
        .collect();

    let writers: Vec<_> = (0..THREADS)
        .map(|thread| {
            let (service, tasks) = (fx.service.clone(), tasks.clone());
            std::thread::spawn(move || {
                for round in 0..ROUNDS {
                    let id = &tasks[(thread + round) % TASKS];
                    if round % 3 == 0 {
                        let title = json!({ "title": format!("Thread {thread}, round {round}") });
                        service.update_task(id, Some(&title)).unwrap();
                    } else {
                        let to = if round % 2 == 0 { "review" } else { "running" };
                        override_and_read(&service, id, to);
                    }
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }

    // Each thread changes each task ROUNDS / TASKS times; each change logs one event.
    for id in &tasks {
        assert_whole(&fx, id, 1 + THREADS * ROUNDS / TASKS);
    }
}

#[test]
fn two_writers_of_one_task_leave_its_file_as_the_last_change_in_memory() {
    let fx = Fixture::new();
    let id = create(&fx.service, "Shared").id;
    let start = Arc::new(Barrier::new(2));

    let hook = {
        let (service, id, start) = (fx.service.clone(), id.clone(), Arc::clone(&start));
        std::thread::spawn(move || {
            start.wait();
            for round in 0..40 {
                let to = if round % 2 == 0 { "review" } else { "running" };
                override_and_read(&service, &id, to);
            }
        })
    };
    let route = {
        let (service, id, start) = (fx.service.clone(), id.clone(), Arc::clone(&start));
        std::thread::spawn(move || {
            start.wait();
            for round in 0..40 {
                let title = json!({ "title": format!("Renamed {round}") });
                service.update_task(&id, Some(&title)).unwrap();
            }
        })
    };
    hook.join().unwrap();
    route.join().unwrap();

    assert_whole(&fx, &id, 1 + 80);
    assert_eq!(fx.service.get_task(&id).unwrap().title, "Renamed 39");
}

#[test]
fn a_task_deleted_while_others_write_it_leaves_no_file_and_no_row() {
    let fx = Fixture::new();
    let id = create(&fx.service, "Doomed").id;
    let start = Arc::new(Barrier::new(THREADS + 1));

    let writers: Vec<_> = (0..THREADS)
        .map(|_| {
            let (service, id, start) = (fx.service.clone(), id.clone(), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                for round in 0..200 {
                    let to = if round % 2 == 0 { "review" } else { "running" };
                    match service.apply_event(&id, Some(&overridden(to))) {
                        Ok(_) => {}
                        Err(TaskError::NotFound(_)) => return,
                        Err(error) => panic!("{error}"),
                    }
                }
            })
        })
        .collect();
    start.wait();
    std::thread::sleep(Duration::from_millis(2));
    fx.service.delete_task(&id).unwrap();
    for writer in writers {
        writer.join().unwrap();
    }

    assert!(matches!(
        fx.service.get_task(&id),
        Err(TaskError::NotFound(_))
    ));
    assert!(!task_file_path(fx.data(), &id).exists());
    assert!(!event_log_path(fx.data(), &id).exists());
    let index = fx.index();
    assert_eq!(index.list_tasks(None).unwrap(), []);
    assert_eq!(index.list_events(&id).unwrap(), []);
    assert!(!index.task_files().unwrap().contains_key(&id));
}

#[test]
fn an_edit_by_hand_between_two_writes_is_still_refused_and_read() {
    let fx = Fixture::new();
    let (_, clock) = manual_clock(0);
    let service = fx.with(Build {
        clock: Some(clock),
        ..Build::default()
    });
    let task = create(&service, "First");
    // The checks of the file wait for the clock, which stands: only the store sees the edit.
    service.init().unwrap();
    service.close();
    let edit_by_hand = |title: &str| {
        let current = service.get_task(&task.id).unwrap();
        let edited = Task {
            title: title.into(),
            ..current
        };
        fs::write(task_file_path(fx.data(), &task.id), serialize_task(&edited)).unwrap();
    };

    override_and_read(&service, &task.id, "review");
    edit_by_hand("Edited by hand, once");
    let refused = service.apply_event(&task.id, Some(&overridden("running")));
    assert!(matches!(refused, Err(TaskError::FileChanged(_))));
    assert_eq!(
        service.get_task(&task.id).unwrap().title,
        "Edited by hand, once"
    );

    override_and_read(&service, &task.id, "running");
    edit_by_hand("Edited by hand, twice");
    let refused = service.update_task(&task.id, Some(&json!({ "title": "Renamed" })));
    assert!(matches!(refused, Err(TaskError::FileChanged(_))));
    assert_eq!(
        service.get_task(&task.id).unwrap().title,
        "Edited by hand, twice"
    );
}

#[test]
fn a_hook_change_whose_file_cannot_be_written_fails_and_keeps_the_task_as_it_was() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    // A folder where the temporary file goes makes the write fail on every platform.
    let mut temporary = task_file_path(fx.data(), &task.id).into_os_string();
    temporary.push(".tmp");
    fs::create_dir(temporary).unwrap();

    let event = overridden("review");
    let failed = fx.service.apply_event(&task.id, Some(&event));

    assert!(matches!(failed, Err(TaskError::Failed(_))));
    assert_eq!(fx.service.get_task(&task.id).unwrap(), task);
    assert_eq!(
        fx.index().list_tasks(None).unwrap(),
        std::slice::from_ref(&task)
    );
    assert_eq!(event_kinds(&fx.service, &task.id), ["task.created"]);
    // The dashboards heard of the change, so they hear that the task is back.
    assert_eq!(last_message(&fx), DaemonMessage::TaskChanged { task });
}
