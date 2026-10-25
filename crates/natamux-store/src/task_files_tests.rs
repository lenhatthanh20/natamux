use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;
use crate::watch::{WATCH_DELAY, watch_task_files};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "natamux-files-{}-{}",
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

fn task() -> Task {
    Task {
        id: "T-0042".into(),
        title: "Fix timezone bug in reminders".into(),
        description: "The reminder fires one hour late.\n\nSee the run notes.".into(),
        state: natamux_proto::tasks::task::TaskState::Running,
        priority: "high".into(),
        labels: vec!["bug".into()],
        agent: Some("claude-code".into()),
        branch: Some("natamux/t-0042-fix-timezone".into()),
        stage: "implement".into(),
        stages: vec![json!("implement"), json!("review"), json!("release")],
        depends_on: vec!["T-0007".into()],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:30:00.000Z".into(),
        ..Task::default()
    }
}

fn ids(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|listed| listed.id.as_str()).collect()
}

#[test]
fn reads_back_exactly_what_it_wrote() {
    let dir = TempDir::new();
    write_task_file(&dir.0, &task()).unwrap();
    assert_eq!(read_task_file(&dir.0, "T-0042").unwrap(), Some(task()));
}

#[test]
fn round_trips_an_empty_description() {
    let empty = Task {
        description: String::new(),
        ..task()
    };
    assert_eq!(parse_task_file(&serialize_task(&empty)).unwrap(), empty);
}

#[test]
fn reads_a_hand_written_file_with_an_unquoted_timestamp() {
    let text = [
        "---",
        "id: T-0001",
        "title: Written by hand",
        "createdAt: 2026-10-08T09:00:00Z",
        "updatedAt: 2026-10-08T09:00:00Z",
        "---",
        "A note.",
    ]
    .join("\n");

    let parsed = parse_task_file(&text).unwrap();

    assert_eq!(parsed.created_at, "2026-10-08T09:00:00.000Z");
    assert_eq!(parsed.description, "A note.");
    assert_eq!(parsed.state.name(), "draft");
}

#[test]
fn loads_a_file_with_a_pr_link_of_another_scheme_without_the_link() {
    let text = [
        "---",
        "id: T-0001",
        "title: Written by hand",
        "pr: vscode://file/etc/passwd",
        "createdAt: 2026-10-08T09:00:00Z",
        "updatedAt: 2026-10-08T09:00:00Z",
        "---",
    ]
    .join("\n");

    assert_eq!(parse_task_file(&text).unwrap().pr, None);
}

#[test]
fn throws_on_a_file_that_is_not_a_valid_task() {
    assert!(parse_task_file("---\nid: nope\n---\n").is_err());
}

#[test]
fn returns_undefined_for_a_task_that_does_not_exist() {
    let dir = TempDir::new();
    assert_eq!(read_task_file(&dir.0, "T-0099").unwrap(), None);
}

#[test]
fn lists_task_ids_in_numeric_order_and_ignores_other_files() {
    let dir = TempDir::new();
    fs::create_dir_all(tasks_dir(&dir.0)).unwrap();
    let names = [
        "T-10.md",
        "T-2.md",
        "T-3.md.tmp",
        "notes.txt",
        "T-x.md",
        "abc-1.md",
        "T42.md",
    ];
    for name in names
        .iter()
        .chain(&["A1-2.md", "TASK-5.md.bak", "TASK-5.md", "ABC-2.md"])
    {
        fs::write(tasks_dir(&dir.0).join(name), "").unwrap();
    }
    assert_eq!(
        list_task_ids(&dir.0).unwrap(),
        ["ABC-2", "T-2", "TASK-5", "T-10"]
    );
}

#[test]
fn lists_no_ids_when_the_data_directory_has_no_tasks_yet() {
    let dir = TempDir::new();
    assert!(list_task_ids(&dir.0.join("missing")).unwrap().is_empty());
}

#[test]
fn lists_every_task_file_as_a_task() {
    let dir = TempDir::new();
    write_task_file(&dir.0, &task()).unwrap();
    write_task_file(
        &dir.0,
        &Task {
            id: "T-0043".into(),
            title: "Second".into(),
            ..task()
        },
    )
    .unwrap();
    assert_eq!(ids(&list_task_files(&dir.0).unwrap()), ["T-0042", "T-0043"]);
}

#[test]
fn lists_tasks_with_ids_the_user_chose_by_the_number_at_the_end_of_the_id() {
    let dir = TempDir::new();
    for id in ["T-0010", "ABC-12", "TASK-0005", "T-0003", "T-0005"] {
        write_task_file(
            &dir.0,
            &Task {
                id: id.into(),
                ..task()
            },
        )
        .unwrap();
    }
    assert_eq!(
        ids(&list_task_files(&dir.0).unwrap()),
        ["T-0003", "T-0005", "TASK-0005", "T-0010", "ABC-12"]
    );
}

#[test]
fn gives_the_time_and_size_of_the_file_it_wrote() {
    let dir = TempDir::new();
    let written = write_task_file(&dir.0, &task()).unwrap();
    let path = task_file_path(&dir.0, &task().id);
    let meta = fs::metadata(&path).unwrap();
    let since = meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap();

    assert_eq!(written.size, meta.len());
    assert_eq!(
        written.mtime_ms,
        since.as_secs() as f64 * 1e3 + f64::from(since.subsec_nanos()) / 1e6
    );
    assert_eq!(stat_file(&path).unwrap(), Some(written));
    assert_eq!(stat_file(&task_file_path(&dir.0, "T-0099")).unwrap(), None);
}

#[test]
fn counts_a_file_as_changed_when_its_time_or_its_size_differs() {
    let stat = |mtime_ms, size| Some(FileStat { mtime_ms, size });
    assert!(same_stat(stat(1.0, 2), stat(1.0, 2)));
    assert!(!same_stat(stat(1.0, 2), stat(1.0, 3)));
    assert!(!same_stat(stat(1.0, 2), stat(4.0, 2)));
    assert!(!same_stat(stat(1.0, 2), None));
    assert!(!same_stat(None, None));
}

#[test]
fn reports_the_ids_of_task_files_that_change_a_burst_of_writes_at_a_time() {
    let dir = TempDir::new();
    fs::create_dir_all(tasks_dir(&dir.0)).unwrap();
    let reports: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    let seen = Arc::clone(&reports);
    let stop = watch_task_files(&dir.0, WATCH_DELAY, move |ids| {
        seen.lock().unwrap().push(ids.into_iter().collect());
    });

    fs::write(tasks_dir(&dir.0).join("T-0043.events.jsonl"), "{}\n").unwrap();
    fs::write(tasks_dir(&dir.0).join("T-0043.md.tmp"), "draft").unwrap();
    for count in 0..5 {
        fs::write(task_file_path(&dir.0, &task().id), format!("edit {count}")).unwrap();
    }

    let until = Instant::now() + Duration::from_secs(2);
    while !reports
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .any(|id| id == "T-0042")
    {
        assert!(Instant::now() < until, "no report of T-0042");
        std::thread::sleep(Duration::from_millis(10));
    }
    let reports = reports.lock().unwrap();
    assert!(reports.iter().flatten().all(|id| id == "T-0042"));
    assert!(reports.len() < 5);
    drop(stop);
}

#[test]
fn makes_the_folder_it_watches_when_there_is_none_yet() {
    let dir = TempDir::new();
    drop(watch_task_files(&dir.0, WATCH_DELAY, |_| {}));
    assert!(stat_file(&tasks_dir(&dir.0)).unwrap().is_some());
}
