//! `remove`, and what it records for the ids.

use std::fs;
use std::sync::{Arc, Mutex};

use natamux_daemon::projects::errors::RepoError;
use serde_json::json;

use crate::{Fx, SharedTasks, ids, input, open, read_json, task, tasks_of, text};

#[test]
fn removes_a_repository_and_keeps_the_others_in_order() {
    let fx = Fx::start();
    fx.repos.add(&input(&fx.git_folder("a"))).unwrap();
    let b = fx.repos.add(&input(&fx.git_folder("b"))).unwrap();
    fx.repos.add(&input(&fx.git_folder("c"))).unwrap();

    fx.repos.remove("b").unwrap();

    assert_eq!(ids(&fx.repos), ["default", "a", "c"]);
    let saved = read_json(&fx.data().join("repos.json"));
    let saved_ids: Vec<&str> = saved
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["id"].as_str().unwrap())
        .collect();
    assert_eq!(saved_ids, ["default", "a", "c"]);
    let reloaded = open(&fx.data(), Some(&fx.dir), None, None);
    assert_eq!(ids(&reloaded), ["default", "a", "c"]);
    assert!(std::path::Path::new(&b.path).exists());
    // The folder of the removed one is kept for its id.
    assert_eq!(
        read_json(&fx.data().join("repos-removed.json")),
        json!({ "b": b.path })
    );
}

#[test]
fn gives_the_same_id_when_the_same_folder_is_added_again_with_the_same_name() {
    let fx = Fx::start();
    let folder = fx.git_folder("web");
    let body = json!({ "path": text(&folder), "name": "Web" });
    let first = fx.repos.add(&body).unwrap();
    fx.repos.remove(&first.id).unwrap();

    assert_eq!(fx.repos.add(&body).unwrap().id, first.id);
}

#[test]
fn does_not_give_the_id_of_a_removed_project_with_tasks_to_another_folder() {
    let fx = Fx::start();
    let tasks: SharedTasks = Arc::new(Mutex::new(vec![task(Some("app"), None)]));
    let with_tasks = open(&fx.data(), Some(&fx.dir), None, Some(tasks_of(&tasks)));
    let first = fx.git_folder("app");
    // With no removed folder on record, the tasks that name the id get the new project.
    assert_eq!(with_tasks.add(&input(&first)).unwrap().id, "app");
    with_tasks.remove("app").unwrap();
    let other = crate::git_init(&fx.root.path().join("other").join("app"));

    assert_eq!(with_tasks.add(&input(&other)).unwrap().id, "app-2");
    assert_eq!(with_tasks.add(&input(&first)).unwrap().id, "app");
}

#[test]
fn removes_the_default_repository_like_any_other_and_refuses_an_unknown_one() {
    let fx = Fx::start();
    assert!(matches!(
        fx.repos.remove("nope"),
        Err(RepoError::NotFound(_))
    ));
    assert_eq!(
        fx.repos.remove("nope").unwrap_err().to_string(),
        "Repository nope is not known. Add it first"
    );

    fx.repos.remove("default").unwrap();

    assert_eq!(fx.repos.list().unwrap(), vec![]);
    assert!(matches!(
        fx.repos.remove("default"),
        Err(RepoError::NotFound(_))
    ));
    assert!(fx.dir.exists());
}

#[test]
fn refuses_to_remove_a_repository_that_still_has_workspaces() {
    let fx = Fx::start();
    let tasks: SharedTasks = Arc::new(Mutex::new(vec![
        task(Some("web"), Some("/w/T-0001")),
        task(Some("web"), Some("/w/T-0002")),
        task(Some("web"), None),
        task(Some("api"), Some("/w/T-0003")),
        task(None, Some("/w/T-0004")),
    ]));
    let with_tasks = open(&fx.data(), Some(&fx.dir), None, Some(tasks_of(&tasks)));
    with_tasks.add(&input(&fx.git_folder("web"))).unwrap();
    with_tasks.add(&input(&fx.git_folder("api"))).unwrap();

    let web = with_tasks.remove("web").unwrap_err();
    assert!(matches!(&web, RepoError::InUse(_)));
    assert_eq!(web.to_string(), "Remove the 2 workspaces of web first");
    assert_eq!(
        with_tasks.remove("api").unwrap_err().to_string(),
        "Remove the 1 workspace of api first"
    );
    assert_eq!(ids(&with_tasks), ["default", "web", "api"]);

    tasks.lock().unwrap().drain(0..2);
    with_tasks.remove("web").unwrap();
    assert_eq!(ids(&with_tasks), ["default", "api"]);
}

#[test]
fn counts_workspaces_of_tasks_without_a_repository_for_the_default_one() {
    let fx = Fx::start();
    let tasks: SharedTasks = Arc::new(Mutex::new(vec![task(None, Some("/w/T-0001"))]));
    let with_tasks = open(&fx.data(), Some(&fx.dir), None, Some(tasks_of(&tasks)));

    assert_eq!(
        with_tasks.remove("default").unwrap_err().to_string(),
        "Remove the 1 workspace of default first"
    );

    tasks.lock().unwrap().pop();
    with_tasks.remove("default").unwrap();
    assert_eq!(with_tasks.list().unwrap(), vec![]);
}

#[test]
fn reads_repos_json_once_and_again_only_after_its_own_save() {
    let fx = Fx::start();
    let file = fx.data().join("repos.json");
    let before = fx.repos.list().unwrap();
    fs::write(&file, "[]\n").unwrap();

    assert_eq!(fx.repos.list().unwrap(), before);
    assert_eq!(fx.repos.path_of(None).unwrap(), text(&fx.dir));

    let added = fx.repos.add(&input(&fx.git_folder("web"))).unwrap();
    let mut expected = before.clone();
    expected.push(added.clone());
    assert_eq!(fx.repos.list().unwrap(), expected);
    fx.repos.remove("default").unwrap();
    assert_eq!(fx.repos.list().unwrap(), vec![added]);
}

#[test]
fn gives_copies_so_a_caller_cannot_change_the_list_it_keeps() {
    let fx = Fx::start();
    let mut got = fx.repos.get(None).unwrap();
    got.name = "changed".into();
    let mut listed = fx.repos.list().unwrap();
    listed.pop();

    let kept = fx.repos.list().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(
        (kept[0].id.as_str(), kept[0].name.as_str()),
        ("default", "repo")
    );
}

#[test]
fn keeps_a_removed_folder_on_record_in_key_order_of_js() {
    // Ids that are whole numbers come first in a JS object, so they do in the file.
    let fx = Fx::start();
    fx.repos
        .add(&json!({ "path": text(&fx.git_folder("b")), "name": "b" }))
        .unwrap();
    fx.repos
        .add(&json!({ "path": text(&fx.git_folder("n")), "name": "7" }))
        .unwrap();
    fx.repos.remove("b").unwrap();
    fx.repos.remove("7").unwrap();

    let file = fs::read_to_string(fx.data().join("repos-removed.json")).unwrap();
    assert!(file.starts_with("{\n  \"7\": "));
    assert!(file.ends_with("}\n"));
}
