//! The default project: `NATAMUX_REPO_DIR` is added once, as "default".

use std::fs;
use std::path::PathBuf;

use natamux_daemon::projects::errors::RepoError;
use natamux_daemon::projects::repos::Repos;
use natamux_daemon::shared::jobs::system_clock;
use natamux_proto::projects::repo::Repo;
use serde_json::json;

use crate::{Fx, git_init, ids, open, read_json, text};

fn repo(id: &str, name: &str, path: &std::path::Path) -> Repo {
    Repo {
        id: id.into(),
        name: name.into(),
        path: text(path),
        worktrees_dir: None,
    }
}

#[test]
fn adds_the_repository_it_starts_with_as_default_and_keeps_it() {
    let fx = Fx::start();

    assert_eq!(
        fx.repos.list().unwrap(),
        vec![repo("default", "repo", &fx.dir)]
    );
    assert_eq!(fx.repos.path_of(None).unwrap(), text(&fx.dir));
    assert_eq!(fx.repos.path_of(Some("default")).unwrap(), text(&fx.dir));
    let saved = read_json(&fx.data().join("repos.json"));
    assert_eq!(
        saved,
        json!([{ "id": "default", "name": "repo", "path": text(&fx.dir) }])
    );
    assert!(fx.data().join("repos-default-added").exists());
}

#[test]
fn adds_the_repository_it_starts_with_only_once_over_restarts() {
    let fx = Fx::start();
    let file = fx.data().join("repos.json");
    let before = fs::read_to_string(&file).unwrap();

    let restarted = open(&fx.data(), Some(&fx.dir), None, None);

    assert_eq!(restarted.list().unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn names_the_repository_after_its_folder_when_the_path_ends_with_a_slash() {
    let fx = Fx::start();
    let data = fx.root.path().join("slash-data");
    let slashed = PathBuf::from(format!("{}/", text(&fx.dir)));

    let started = open(&data, Some(&slashed), None, None);

    let first = started.list().unwrap().remove(0);
    assert_eq!(
        (first.id.as_str(), first.name.as_str()),
        ("default", "repo")
    );
}

#[test]
fn does_not_stop_when_the_repository_it_starts_with_is_gone() {
    let fx = Fx::start();
    let data = fx.root.path().join("gone-data");
    let gone = fx.root.path().join("gone");

    let started = open(&data, Some(&gone), None, None);

    assert_eq!(
        started.list().unwrap(),
        vec![repo("default", "gone", &gone)]
    );
}

#[test]
fn adds_nothing_when_the_marker_is_there_but_repos_json_is_not() {
    let fx = Fx::start();
    let data = fx.root.path().join("marked-data");
    fs::create_dir(&data).unwrap();
    fs::write(
        data.join("repos-default-added"),
        format!("{}\n", text(&fx.dir)),
    )
    .unwrap();

    assert_eq!(
        open(&data, Some(&fx.dir), None, None).list().unwrap(),
        vec![]
    );
}

#[test]
fn starts_with_no_repositories_when_it_is_given_none() {
    let fx = Fx::start();
    let data = fx.root.path().join("empty-data");
    let empty = open(&data, None, None, None);

    assert_eq!(empty.list().unwrap(), vec![]);
    let error = empty.get(None).unwrap_err();
    assert!(matches!(
        error,
        natamux_daemon::tasks::errors::TaskError::RepoNotFound(_)
    ));
    assert!(error.to_string().contains("Add a project first"));
    assert!(!data.join("repos.json").exists());
    assert!(!data.join("repos-default-added").exists());
}

#[test]
fn adds_the_repository_it_starts_with_at_a_later_start_that_names_one() {
    let fx = Fx::start();
    let data = fx.root.path().join("later-data");
    assert_eq!(open(&data, None, None, None).list().unwrap(), vec![]);

    let later = open(&data, Some(&fx.dir), None, None);

    assert_eq!(ids(&later), ["default"]);
}

#[test]
fn adds_the_repository_it_starts_with_before_the_ones_added_in_an_older_version() {
    let fx = Fx::start();
    let data = fx.root.path().join("old-data");
    fs::create_dir(&data).unwrap();
    let web = git_init(&fx.root.path().join("web"));
    fs::write(
        data.join("repos.json"),
        json!([{ "id": "web", "name": "web", "path": text(&web) }]).to_string(),
    )
    .unwrap();

    let upgraded = open(&data, Some(&fx.dir), None, None);

    assert_eq!(ids(&upgraded), ["default", "web"]);
    assert_eq!(upgraded.list().unwrap()[1], repo("web", "web", &web));
    assert!(data.join("repos-default-added").exists());
}

#[test]
fn does_not_add_the_repository_it_starts_with_again_once_it_is_removed() {
    let fx = Fx::start();
    fx.repos.remove("default").unwrap();
    assert_eq!(fx.repos.list().unwrap(), vec![]);

    let restarted = open(&fx.data(), Some(&fx.dir), None, None);

    assert_eq!(restarted.list().unwrap(), vec![]);
    let error = restarted.path_of(None).unwrap_err();
    assert!(error.to_string().contains("Add a project first"));
}

#[test]
fn does_not_add_the_repository_it_starts_with_when_its_folder_is_added_already() {
    let fx = Fx::start();
    let data = fx.root.path().join("known-data");
    fs::create_dir(&data).unwrap();
    fs::write(
        data.join("repos.json"),
        json!([{ "id": "mine", "name": "Mine", "path": text(&fx.dir) }]).to_string(),
    )
    .unwrap();

    let known = open(&data, Some(&fx.dir), None, None);

    assert_eq!(ids(&known), ["mine"]);
}

#[test]
fn stops_the_start_only_when_repos_json_cannot_be_read() {
    // Regression test of the start wiring: a broken file is an error, a missing folder is not.
    let fx = Fx::start();
    let data = fx.root.path().join("broken-data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("repos.json"), "not json").unwrap();

    let options = natamux_daemon::projects::repos::RepoOptions {
        data_dir: data,
        default_path: Some(fx.dir.clone()),
        worktrees_dir: None,
        tasks: None,
        now: system_clock(),
    };

    assert!(matches!(
        Repos::with_options(options),
        Err(RepoError::Failed(_))
    ));
}
