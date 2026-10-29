use futures::channel::mpsc::UnboundedReceiver;

use super::*;

fn code(error: ClientError) -> (u16, String) {
    match error {
        ClientError::Status { status, message } => (status, message),
        other => panic!("not a status error: {other}"),
    }
}

fn add(path: &str, name: Option<&str>, dir: Option<&str>) -> AddRepo {
    AddRepo {
        path: path.into(),
        name: name.map(Into::into),
        worktrees_dir: dir.map(Into::into),
    }
}

fn watch(fake: &FakeDaemon) -> UnboundedReceiver<natamux_client::DashboardEvent> {
    let (mut receiver, _feed) = fake.watch();
    while receiver.try_recv().is_ok() {}
    receiver
}

#[test]
fn a_relative_folder_is_refused() {
    let error = FakeDaemon::default().add_repo(&add("work/app", None, None));
    assert_eq!(
        code(error.unwrap_err()),
        (400, "work/app is not an absolute path to a folder".into())
    );
}

#[test]
fn a_known_folder_is_added_already() {
    let error = FakeDaemon::default().add_repo(&add("/work/natamux", None, None));
    assert_eq!(
        code(error.unwrap_err()),
        (400, "/work/natamux is added already".into())
    );
}

#[test]
fn the_id_comes_from_the_folder_name_and_a_clash_gets_a_number() {
    let fake = FakeDaemon::default();
    let first = fake.add_repo(&add("/srv/My Shop", None, None)).unwrap();
    assert_eq!(
        (first.id.as_str(), first.name.as_str()),
        ("my-shop", "My Shop")
    );
    let second = fake.add_repo(&add("/other/my-shop", None, None)).unwrap();
    assert_eq!(second.id, "my-shop-2");
}

#[test]
fn a_name_and_an_own_worktree_folder_are_kept() {
    let repo = FakeDaemon::default()
        .add_repo(&add("/srv/app", Some("Store"), Some("/w")))
        .unwrap();
    assert_eq!((repo.id.as_str(), repo.name.as_str()), ("store", "Store"));
    assert_eq!(repo.worktrees_dir.as_deref(), Some("/w"));
}

#[test]
fn a_relative_worktree_folder_is_named_in_the_error() {
    let error = FakeDaemon::default().add_repo(&add("/srv/app", None, Some("w")));
    assert!(
        code(error.unwrap_err())
            .1
            .starts_with("The worktree folder ")
    );
}

#[test]
fn adding_a_project_sends_repos_changed() {
    let fake = FakeDaemon::default();
    let mut feed = watch(&fake);
    fake.add_repo(&add("/srv/app", None, None)).unwrap();
    match feed.try_recv().unwrap() {
        natamux_client::DashboardEvent::Repos { repos, .. } => {
            assert_eq!(repos.last().map(|repo| repo.id.as_str()), Some("app"));
        }
        other => panic!("not repos: {other:?}"),
    }
}

#[test]
fn the_daemon_has_a_worktree_folder() {
    assert!(FakeDaemon::default().data().worktrees_dir.is_some());
}

#[test]
fn natamux_has_three_open_issues_and_one_is_imported() {
    let issues = FakeDaemon::default().list_issues(Some("natamux")).unwrap();
    let numbers: Vec<u64> = issues.iter().map(|item| item.number).collect();
    assert_eq!(numbers, [12, 15, 18]);
    assert_eq!(issues[1].task_id.as_deref(), Some("T-0141"));
    assert_eq!(issues[0].labels, ["feature"]);
}

#[test]
fn a_missing_gh_answers_409_on_both_calls() {
    let fake = FakeDaemon::default();
    fake.set_gh_missing(true);
    let message = "GitHub CLI failed: gh is not installed or not on the PATH";
    assert_eq!(
        code(fake.list_issues(None).unwrap_err()),
        (409, message.into())
    );
    assert_eq!(
        code(fake.import_issues(&[12], None).unwrap_err()),
        (409, message.into())
    );
}

#[test]
fn import_makes_a_draft_task_and_skips_imported_issues() {
    let fake = FakeDaemon::default();
    let tasks = fake.import_issues(&[12, 15], Some("natamux")).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Show stock levels on product pages");
    assert_eq!(tasks[0].labels, ["feature"]);
    assert_eq!(tasks[0].repo.as_deref(), Some("natamux"));
    assert_eq!(
        tasks[0].description,
        "Details of issue 12.\n\nImported from https://github.com/natamux/natamux/issues/12"
    );
    let issues = fake.list_issues(Some("natamux")).unwrap();
    assert_eq!(issues[0].task_id.as_deref(), Some(tasks[0].id.as_str()));
}

#[test]
fn seeded_projects_are_approved_and_an_added_one_waits() {
    let fake = FakeDaemon::default();
    assert!(fake.repo_trust("natamux").unwrap().trusted);
    let repo = fake.add_repo(&add("/srv/app", None, None)).unwrap();
    let view = fake.repo_trust(&repo.id).unwrap();
    assert!(!view.trusted);
    assert_eq!(view.hash, "fake-app");
    assert!(!view.commands.is_empty());
    assert_eq!(view.setup[0].file, ".natamux-setup");
}

#[test]
fn approving_the_shown_hash_trusts_and_sends_repos_changed() {
    let fake = FakeDaemon::default();
    let repo = fake.add_repo(&add("/srv/app", None, None)).unwrap();
    let mut feed = watch(&fake);
    let hash = fake.repo_trust(&repo.id).unwrap().hash;
    assert!(fake.approve_repo(&repo.id, &hash).unwrap().trusted);
    assert!(fake.repo_trust(&repo.id).unwrap().trusted);
    assert!(matches!(
        feed.try_recv().unwrap(),
        natamux_client::DashboardEvent::Repos { .. }
    ));
}

#[test]
fn an_old_hash_answers_409_and_an_unknown_project_404() {
    let fake = FakeDaemon::default();
    fake.distrust_repo("natamux");
    let error = fake.approve_repo("natamux", "old").unwrap_err();
    assert_eq!(code(error).0, 409);
    assert!(!fake.repo_trust("natamux").unwrap().trusted);
    let error = fake.repo_trust("nope").unwrap_err();
    assert_eq!(code(error), (404, "Project nope not found".into()));
}
