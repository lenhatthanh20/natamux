use super::*;

#[test]
fn an_entry_of_the_tasks_folder_names_its_task_exactly() {
    for (name, task) in [
        ("T-1.md", Some("T-1")),
        ("T-1", Some("T-1")),
        ("T-1.screens", Some("T-1")),
        ("T-1.md.tmp", Some("T-1")),
        ("T-10.md", Some("T-10")),
        ("ABC-42.events.jsonl", Some("ABC-42")),
        ("T-1x.md", None),
        ("t-1.md", None),
        ("templates", None),
        ("-1.md", None),
    ] {
        assert_eq!(task_of_task_entry(name), task, "{name}");
    }
}

#[test]
fn a_name_that_is_not_a_task_id_is_refused() {
    let dir = std::env::temp_dir();
    assert!(task_paths(&dir, "../x").is_err());
    assert!(mark_deleting(&dir, "T-1/../../x").is_err());
}

#[test]
fn the_sandbox_folder_of_an_isolated_task_goes_with_it() {
    let data = std::env::temp_dir().join(format!("natamux-purge-sandbox-{}", std::process::id()));
    let own = data.join("sandbox/T-7/tmp");
    let other = data.join("sandbox/T-70");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(own.join("PROMPT-MARK"), "x").unwrap();
    assert_eq!(
        task_paths(&data, "T-7").unwrap(),
        [data.join("sandbox/T-7")]
    );
    purge_task_files(&data, "T-7").unwrap();
    assert!(!data.join("sandbox/T-7").exists());
    assert!(other.exists());
    std::fs::remove_dir_all(&data).unwrap();
}
