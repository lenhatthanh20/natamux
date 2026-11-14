use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::task_files::{stat_file, task_file_path};

fn make_task(id: &str) -> Task {
    Task {
        id: id.into(),
        title: "Example".into(),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn disk() -> Disk {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
        "natamux-writes-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let index = TaskIndex::open(Path::new(":memory:")).unwrap();
    Disk::new(folder, index)
}

#[test]
fn a_write_waits_for_every_turn_before_its_own() {
    let slot = Arc::new(TaskSlot::default());
    let first = slot.ticket();
    let second = slot.ticket();
    let turn = slot.wait_turn(first);
    let ran = Arc::new(AtomicBool::new(false));
    let waiter = {
        let (slot, ran) = (Arc::clone(&slot), Arc::clone(&ran));
        std::thread::spawn(move || {
            let _turn = slot.wait_turn(second);
            ran.store(true, Ordering::SeqCst);
        })
    };

    std::thread::sleep(Duration::from_millis(50));
    assert!(!ran.load(Ordering::SeqCst));
    drop(turn);
    waiter.join().unwrap();

    assert!(ran.load(Ordering::SeqCst));
    slot.wait_idle();
}

#[test]
fn a_job_dropped_before_it_ran_still_ends_its_turn() {
    let slot = Arc::new(TaskSlot::default());
    let job = WriteJob::new(Arc::clone(&slot), make_task("T-0001"), Vec::new(), true);

    drop(job);

    // Would wait for ever if the turn had not ended.
    slot.wait_idle();
    let next = slot.ticket();
    drop(slot.wait_turn(next));
}

#[test]
fn a_write_keeps_the_stat_of_the_file_it_made_and_its_index_row() {
    let disk = disk();
    let slot = Arc::new(TaskSlot::default());
    let task = make_task("T-0001");
    let job = WriteJob::new(Arc::clone(&slot), task.clone(), Vec::new(), true);

    assert!(job.run(&disk).is_ok());

    slot.wait_idle();
    let on_disk = stat_file(&task_file_path(&disk.data_dir, "T-0001")).unwrap();
    assert_eq!(slot.stat(), on_disk);
    assert_eq!(disk.index().list_tasks(None).unwrap(), [task]);
    std::fs::remove_dir_all(&disk.data_dir).ok();
}

#[test]
fn a_file_that_cannot_be_written_fails_with_no_stat_and_no_row() {
    let disk = disk();
    let slot = Arc::new(TaskSlot::default());
    // A folder where the temporary file goes makes the write fail on every platform.
    let mut temporary = task_file_path(&disk.data_dir, "T-0001").into_os_string();
    temporary.push(".tmp");
    std::fs::create_dir_all(temporary).unwrap();
    let job = WriteJob::new(Arc::clone(&slot), make_task("T-0001"), Vec::new(), true);

    assert!(job.run(&disk).is_err());

    slot.wait_idle();
    assert_eq!(slot.stat(), None);
    assert_eq!(disk.index().list_tasks(None).unwrap(), []);
    std::fs::remove_dir_all(&disk.data_dir).ok();
}
