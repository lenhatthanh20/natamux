//! `add`, the worktree folders and the files `list` reads.

use std::fs;
use std::path::{Path, PathBuf};

use natamux_daemon::projects::errors::RepoError;
use natamux_daemon::projects::repos::Repos;
use serde_json::{Value, json};

use crate::{Fx, git_init, ids, input, open, read_json, text};

fn worktrees_of(fx: &Fx, repos: &Repos, id: Option<&str>) -> PathBuf {
    repos
        .worktrees_dir_of(id)
        .or_else(|| repos.worktrees_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| fx.root.path().join("no-folder"))
}

#[test]
fn gives_a_new_project_an_id_from_its_name_not_default() {
    let fx = Fx::start();
    let empty = open(&fx.root.path().join("empty-data"), None, None, None);

    let added = empty
        .add(&input(&fx.git_folder("Mon dépôt avec espaces")))
        .unwrap();

    assert_eq!(added.id, "mon-d-p-t-avec-espaces");
    assert_eq!(added.name, "Mon dépôt avec espaces");
    let path = empty.path_of(Some("mon-d-p-t-avec-espaces")).unwrap();
    assert_eq!(path, added.path);
}

#[test]
fn refuses_a_folder_that_is_gone_and_keeps_the_list_as_it_was() {
    let fx = Fx::start();
    let file = fx.data().join("repos.json");
    let before = fs::read_to_string(&file).unwrap();

    let error = fx.repos.add(&input(&fx.root.path().join("deleted"))).unwrap_err();

    assert!(matches!(error, RepoError::Input(_)));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn gives_default_back_to_its_folder_when_it_is_added_again() {
    let fx = Fx::start();
    fx.repos.remove("default").unwrap();

    let again = fx.repos.add(&input(&fx.dir)).unwrap();

    assert_eq!(again.id, "default");
    assert_eq!(fx.repos.path_of(None).unwrap(), again.path);
}

#[test]
fn adds_a_repository_keeps_it_and_makes_the_id_unique() {
    let fx = Fx::start();
    let first = fx
        .repos
        .add(&json!({ "path": text(&fx.git_folder("web")), "name": "Web" }))
        .unwrap();
    let second = fx
        .repos
        .add(&json!({ "path": text(&fx.git_folder("web-2-src")), "name": "Web" }))
        .unwrap();

    assert_eq!(first.id, "web");
    assert_eq!(second.id, "web-2");
    assert_eq!(ids(&fx.repos), ["default", "web", "web-2"]);
    let reloaded = open(&fx.data(), Some(&fx.dir), None, None);
    assert_eq!(reloaded.list().unwrap().len(), 3);
}

#[test]
fn names_a_repository_after_its_folder_by_default() {
    let fx = Fx::start();

    assert_eq!(fx.repos.add(&input(&fx.git_folder("api"))).unwrap().name, "api");
}

#[test]
fn keeps_the_worktree_folder_of_a_project_and_makes_the_folder() {
    let fx = Fx::start();
    let trees = fx.root.path().join("trees").join("web");

    let added = fx
        .repos
        .add(&json!({ "path": text(&fx.git_folder("web")), "worktreesDir": text(&trees) }))
        .unwrap();

    assert_eq!(added.worktrees_dir, Some(text(&trees)));
    assert!(trees.exists());
    let reloaded = open(&fx.data(), Some(&fx.dir), None, None);
    assert_eq!(reloaded.get(Some("web")).unwrap().worktrees_dir, Some(text(&trees)));
}

#[test]
fn keeps_no_worktree_folder_when_it_is_the_default_one_or_empty() {
    let fx = Fx::start();
    let own = fx.root.path().join("worktrees");
    let with_default = open(&fx.root.path().join("tree-data"), None, Some(&own), None);

    let same = with_default
        .add(&json!({ "path": text(&fx.git_folder("web")), "worktreesDir": format!("{}/", text(&own)) }))
        .unwrap();
    let empty = with_default
        .add(&json!({ "path": text(&fx.git_folder("api")), "worktreesDir": "  " }))
        .unwrap();

    assert_eq!(same.worktrees_dir, None);
    assert_eq!(empty.worktrees_dir, None);
    assert_eq!(with_default.worktrees_dir(), Some(own.as_path()));
}

#[test]
fn refuses_a_worktree_folder_that_is_relative_or_cannot_be_made_and_adds_nothing() {
    let fx = Fx::start();

    let relative = fx
        .repos
        .add(&json!({ "path": text(&fx.git_folder("web")), "worktreesDir": "trees/web" }))
        .unwrap_err();
    assert!(matches!(&relative, RepoError::Input(_)));
    assert_eq!(
        relative.to_string(),
        "The worktree folder trees/web is not an absolute path"
    );

    // A folder cannot be made under a file; checked on unix, where that is how it fails.
    #[cfg(unix)]
    {
        let file = fx.root.path().join("a-file");
        fs::write(&file, "not a folder\n").unwrap();
        let blocked = fx
            .repos
            .add(&json!({
                "path": text(&fx.git_folder("api")),
                "worktreesDir": text(&file.join("trees")),
            }))
            .unwrap_err();
        assert!(matches!(&blocked, RepoError::Input(_)));
        assert!(blocked.to_string().contains("cannot be made"));
    }
    assert_eq!(ids(&fx.repos), ["default"]);
}

#[test]
fn gives_a_projects_own_worktree_folder_or_else_the_daemons() {
    let fx = Fx::start();
    let own = fx.root.path().join("worktrees");
    let with_default = open(
        &fx.root.path().join("tree-data"),
        Some(&fx.dir),
        Some(&own),
        None,
    );
    let trees = fx.root.path().join("trees").join("web");
    with_default
        .add(&json!({ "path": text(&fx.git_folder("web")), "worktreesDir": text(&trees) }))
        .unwrap();

    assert_eq!(worktrees_of(&fx, &with_default, Some("web")), trees);
    assert_eq!(worktrees_of(&fx, &with_default, Some("default")), own);
    assert_eq!(worktrees_of(&fx, &with_default, None), own);
    assert_eq!(worktrees_of(&fx, &with_default, Some("nope")), own);
}

#[test]
fn reads_a_repos_json_of_an_older_version_and_refuses_a_relative_worktree_folder_in_it() {
    let fx = Fx::start();
    let data = fx.root.path().join("old-data");
    fs::create_dir(&data).unwrap();
    let file = data.join("repos.json");
    let trees = fx.root.path().join("trees");
    let own = fx.root.path().join("worktrees");
    fs::write(
        &file,
        json!([
            { "id": "old", "name": "old", "path": text(&fx.dir) },
            { "id": "new", "name": "new", "path": text(&fx.root.path().join("new")), "worktreesDir": text(&trees) },
        ])
        .to_string(),
    )
    .unwrap();
    let old = open(&data, None, Some(&own), None);

    assert_eq!(ids(&old), ["old", "new"]);
    assert_eq!(worktrees_of(&fx, &old, Some("old")), own);
    assert_eq!(worktrees_of(&fx, &old, Some("new")), trees);

    fs::write(
        &file,
        json!([{ "id": "x", "name": "x", "path": text(&fx.dir), "worktreesDir": "x" }]).to_string(),
    )
    .unwrap();
    assert!(open(&data, None, None, None).list().is_err());
}

#[test]
fn refuses_a_folder_inside_a_repository_a_repeat_and_an_unknown_id() {
    let fx = Fx::start();
    let inside = fx.dir.join("sub");
    fs::create_dir(&inside).unwrap();

    let sub = fx.repos.add(&input(&inside)).unwrap_err();
    assert!(matches!(&sub, RepoError::Input(_)));
    assert!(sub.to_string().contains("is inside a repository; give its top folder"));
    let repeat = fx.repos.add(&input(&fx.dir)).unwrap_err();
    assert!(matches!(&repeat, RepoError::Input(_)));
    assert!(repeat.to_string().ends_with("is added already"));
    assert!(fx.repos.get(Some("nope")).is_err());
}

#[test]
fn gives_default_only_to_the_folder_first_added_as_default() {
    let fx = Fx::start();
    let empty = open(&fx.root.path().join("empty-data"), None, None, None);

    assert_eq!(empty.add(&input(&fx.git_folder("Default"))).unwrap().id, "default-2");
}

#[test]
fn keeps_the_empty_list_of_a_missing_repos_json_and_makes_the_file_on_save() {
    let fx = Fx::start();
    let data = fx.root.path().join("missing-data");
    let file = data.join("repos.json");
    let empty = open(&data, None, None, None);
    assert_eq!(empty.list().unwrap(), vec![]);
    fs::create_dir(&data).unwrap();
    fs::write(
        &file,
        json!([{ "id": "other", "name": "other", "path": text(&fx.dir) }]).to_string(),
    )
    .unwrap();

    assert_eq!(empty.list().unwrap(), vec![]);

    let added = empty.add(&input(&fx.git_folder("web"))).unwrap();
    assert_eq!(empty.list().unwrap(), vec![added.clone()]);
    assert_eq!(read_json(&file), json!([serde_json::to_value(&added).unwrap()]));
}

#[test]
fn saves_repos_json_as_node_does() {
    // `JSON.stringify(repos, null, 2) + "\n"`, byte for byte, with a worktree folder.
    let fx = Fx::start();
    let trees = fx.root.path().join("trees");
    let added = fx
        .repos
        .add(&json!({ "path": text(&git_init(&fx.root.path().join("web"))), "worktreesDir": text(&trees) }))
        .unwrap();

    let expected = format!(
        "[\n  {{\n    \"id\": \"default\",\n    \"name\": \"repo\",\n    \"path\": {}\n  }},\n  {{\n    \"id\": \"web\",\n    \"name\": \"web\",\n    \"path\": {},\n    \"worktreesDir\": {}\n  }}\n]\n",
        Value::String(text(&fx.dir)),
        Value::String(added.path.clone()),
        Value::String(text(&trees)),
    );
    assert_eq!(fs::read_to_string(fx.data().join("repos.json")).unwrap(), expected);
}

#[test]
fn refuses_input_that_breaks_the_schema_with_zods_issues() {
    let fx = Fx::start();

    let error = fx
        .repos
        .add(&json!({ "path": text(&fx.dir), "worktreesDir": 42 }))
        .unwrap_err();
    let RepoError::Invalid(issues) = error else {
        panic!("not a zod error");
    };
    assert_eq!(
        issues.describe(),
        "worktreesDir: Invalid input: expected string, received number"
    );
    let blank = fx.repos.add(&json!({ "path": "   " })).unwrap_err();
    assert_eq!(
        blank.to_string(),
        "path: Too small: expected string to have >=1 characters"
    );
}
