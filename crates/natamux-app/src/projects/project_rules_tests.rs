use super::*;

#[test]
fn accepts_full_paths_and_an_empty_field() {
    for path in [
        "",
        "/work/repo",
        "~/repo",
        "C:\\work\\repo",
        "D:/repo",
        "\\\\server\\share",
    ] {
        assert_eq!(repo_path_problem(path), None, "{path}");
    }
}

#[test]
fn asks_for_the_full_path() {
    for path in ["repo", "../repo", "C:repo"] {
        assert!(
            repo_path_problem(path).is_some_and(|text| text.contains("full path")),
            "{path}"
        );
    }
}

#[test]
fn starts_with_the_default_folder_and_sends_none_when_it_is_not_changed() {
    let shown = shown_worktree(None, Some("/data/worktrees"));
    assert_eq!(shown, "/data/worktrees");
    assert_eq!(
        add_args("/work/web", "", &shown, Some("/data/worktrees")),
        ("/work/web".to_owned(), None, None)
    );
}

#[test]
fn sends_a_changed_folder_then_goes_back_to_the_default() {
    assert_eq!(
        add_args("/work/web", "", "/trees/web", Some("/data/worktrees")),
        ("/work/web".to_owned(), None, Some("/trees/web".to_owned()))
    );
    // After a success the typed value is cleared, so the default shows again.
    assert_eq!(
        shown_worktree(None, Some("/data/worktrees")),
        "/data/worktrees"
    );
}

#[test]
fn trims_the_fields_and_sends_the_name_when_set() {
    assert_eq!(
        add_args("  /work/web ", " Web ", "  ", None),
        ("/work/web".to_owned(), Some("Web".to_owned()), None)
    );
}

#[test]
fn refuses_a_relative_folder_and_adds_nothing() {
    // The dialog sends nothing while either field has a problem.
    assert!(repo_path_problem("trees").is_some_and(|text| text.contains("full path")));
}

#[test]
fn shows_a_worktree_folder_error_from_the_daemon_under_its_field() {
    assert_eq!(
        field_of_error("The worktree folder /a-file/trees cannot be made: ENOTDIR"),
        RepoField::Worktree
    );
    assert_eq!(field_of_error("Not a git repository"), RepoField::Folder);
}

#[test]
fn fills_in_the_default_folder_when_it_comes_after_the_dialog_opened() {
    assert_eq!(shown_worktree(None, None), "");
    assert_eq!(
        shown_worktree(None, Some("/data/worktrees")),
        "/data/worktrees"
    );
    assert_eq!(
        shown_worktree(Some("/mine"), Some("/data/worktrees")),
        "/mine"
    );
    // An emptied field stays empty: it is not the default again.
    assert_eq!(shown_worktree(Some(""), Some("/data/worktrees")), "");
}
