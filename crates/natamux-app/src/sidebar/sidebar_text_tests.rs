use natamux_proto::events::task_event::TaskEvent;
use serde_json::json;

use super::*;

fn labels() -> Vec<String> {
    ["Fix login", "Add export", "Add import", "Zoom"]
        .map(str::to_owned)
        .to_vec()
}

fn task(state: TaskState) -> Task {
    Task {
        id: "T-0001".into(),
        title: "Example".into(),
        state,
        branch: Some("natamux/t-0001-example".into()),
        workspace: Some("/work/T-0001".into()),
        ..Task::default()
    }
}

fn asked(message: &str) -> StoredEvent {
    let event: TaskEvent = serde_json::from_value(json!({
        "taskId": "T-0001", "source": "agent", "type": "agent.asked", "at": "",
        "payload": {"message": message}
    }))
    .unwrap();
    StoredEvent {
        seq: 1,
        accepted: true,
        event,
    }
}

fn labels_of<C: Copy>(items: &[MenuItem<C>]) -> Vec<&'static str> {
    items
        .iter()
        .map(|menu_item| match menu_item {
            MenuItem::Item { label, .. } => *label,
            MenuItem::Separator => "-",
        })
        .collect()
}

// lib/__tests__/roving.test.ts

#[test]
fn finds_the_first_label_that_starts_with_the_text_after_the_current_one() {
    assert_eq!(type_ahead_target(&labels(), "a", 0), Some(1));
}

#[test]
fn goes_to_the_next_match_when_the_same_letter_is_pressed_again() {
    assert_eq!(type_ahead_target(&labels(), "a", 1), Some(2));
    assert_eq!(type_ahead_target(&labels(), "a", 2), Some(1));
}

#[test]
fn keeps_the_current_item_while_a_longer_word_still_matches_it() {
    assert_eq!(type_ahead_target(&labels(), "add i", 1), Some(2));
    assert_eq!(type_ahead_target(&labels(), "add e", 1), Some(1));
}

#[test]
fn returns_1_when_nothing_matches() {
    assert_eq!(type_ahead_target(&labels(), "q", 0), None);
    assert_eq!(type_ahead_target(&[], "a", 0), None);
}

#[test]
fn starts_a_new_word_after_a_pause() {
    assert_eq!(typed_word("ad", 1000, 1500, "d"), "add");
    assert_eq!(typed_word("ad", 1000, 1601, "z"), "z");
}

// lib/__tests__/workspace.test.ts

#[test]
fn quotes_the_question_or_says_where_to_answer() {
    let asked = |question: Option<&str>| WaitReason::Asked {
        question: question.map(str::to_owned),
    };
    assert_eq!(
        banner_text(&asked(Some("Which runner?"))),
        "The agent asks: Which runner?"
    );
    assert_eq!(
        banner_text(&asked(None)),
        "The agent is waiting for your answer in its terminal."
    );
    assert_eq!(row_text(&asked(None)), "Waiting for your answer");
}

#[test]
fn shows_the_reasons_of_a_failed_check_and_not_a_failed_try_that_the_agent_fixes() {
    let failed = WaitReason::CheckFailed {
        reasons: vec!["no tests".into(), "lint".into()],
    };
    assert_eq!(banner_text(&failed), "The checks failed: no tests; lint");
    assert_eq!(
        banner_text(&WaitReason::CheckFailed { reasons: vec![] }),
        "The checks of this stage failed."
    );
}

#[test]
fn says_the_agent_stopped_when_nothing_asked_since_it_started() {
    assert_eq!(
        banner_text(&WaitReason::Stopped),
        "The agent ended its turn. Reply in the terminal, or resume it."
    );
}

// lib/__tests__/pull-request.test.ts

#[test]
fn reads_the_number_from_the_address() {
    assert_eq!(pull_request_number("https://github.com/o/r/pull/12"), "#12");
    assert_eq!(pull_request_number("https://example.com/x"), "Pull request");
}

#[test]
fn says_the_state_and_the_checks() {
    let status = |state: &str, checks: &str| PullRequestStatus {
        state: state.into(),
        checks: checks.into(),
    };
    assert_eq!(pull_request_summary(None), None);
    assert_eq!(
        pull_request_summary(Some(&status("open", "failing"))).as_deref(),
        Some("open, checks fail")
    );
    assert_eq!(
        pull_request_summary(Some(&status("merged", "none"))).as_deref(),
        Some("merged")
    );
}

// Sidebar rules of WorkspaceList.tsx and WorkspaceRow.tsx

#[test]
fn offers_add_subtask_only_to_a_task_with_no_parent() {
    let parent = task(TaskState::Running);
    assert!(labels_of(&row_menu(&parent, false)).contains(&"Add subtask…"));
    let child = Task {
        parent_id: Some("T-0009".into()),
        ..task(TaskState::Running)
    };
    assert!(!labels_of(&row_menu(&child, false)).contains(&"Add subtask…"));
}

#[test]
fn row_menu_puts_the_destructive_items_last() {
    assert_eq!(
        labels_of(&row_menu(&task(TaskState::Done), true)),
        [
            "View changes",
            "Rename",
            "Add subtask…",
            "Unpin",
            "Copy branch name",
            "Copy folder path",
            "-",
            "Remove workspace",
            "Delete task"
        ]
    );
}

#[test]
fn offers_view_changes_only_with_a_workspace() {
    let no_workspace = Task {
        workspace: None,
        ..task(TaskState::Draft)
    };
    assert!(!labels_of(&row_menu(&no_workspace, false)).contains(&"View changes"));
    assert_eq!(
        labels_of(&row_menu(&task(TaskState::Running), false))[0],
        "View changes"
    );
}

#[test]
fn copy_branch_name_is_off_without_a_branch() {
    let no_branch = Task {
        branch: None,
        ..task(TaskState::Draft)
    };
    let copy = row_menu(&no_branch, false)
        .into_iter()
        .find(|menu_item| {
            matches!(
                menu_item,
                MenuItem::Item {
                    command: RowCommand::CopyBranch,
                    ..
                }
            )
        })
        .unwrap();
    assert!(matches!(copy, MenuItem::Item { disabled: true, .. }));
}

fn project_input() -> ProjectMenuInput {
    ProjectMenuInput {
        known: true,
        blurred: false,
        finished: 0,
        removing_finished: false,
        workspaces: 0,
    }
}

#[test]
fn project_menu_offers_remove_finished_only_while_the_group_has_some() {
    assert_eq!(
        labels_of(&project_menu(project_input())),
        [
            "Start a task",
            "Settings",
            "Blur names",
            "-",
            "Remove project"
        ]
    );
    let with_finished = ProjectMenuInput {
        finished: 2,
        blurred: true,
        ..project_input()
    };
    assert_eq!(
        labels_of(&project_menu(with_finished)),
        [
            "Start a task",
            "Settings",
            "Show names",
            "-",
            "Remove finished workspaces",
            "-",
            "Remove project"
        ]
    );
}

#[test]
fn remove_project_is_off_while_the_project_has_workspaces() {
    let busy = ProjectMenuInput {
        workspaces: 1,
        ..project_input()
    };
    let last = project_menu(busy).pop().unwrap();
    assert!(matches!(
        last,
        MenuItem::Item {
            command: ProjectCommand::RemoveProject,
            disabled: true,
            hint: Some("Remove its workspaces first"),
            ..
        }
    ));
}

#[test]
fn a_project_that_is_gone_only_has_the_actions_on_its_workspaces() {
    let gone = ProjectMenuInput {
        known: false,
        finished: 1,
        removing_finished: true,
        ..project_input()
    };
    let items = project_menu(gone);
    assert_eq!(
        labels_of(&items),
        ["Blur names", "-", "Remove finished workspaces"]
    );
    assert!(matches!(items[2], MenuItem::Item { disabled: true, .. }));
}

#[test]
fn names_a_blurred_project_this_project() {
    assert_eq!(shown_name("natamux", true), "this project");
    assert_eq!(shown_name("natamux", false), "natamux");
    assert_eq!(workspace_count(1), "1 workspace");
    assert_eq!(workspace_count(3), "3 workspaces");
}

#[test]
fn a_card_shows_the_question_or_waits_for_it() {
    let waiting = task(TaskState::NeedsInput);
    assert_eq!(
        card_line(&waiting, None).as_deref(),
        Some("Waiting for your answer")
    );
    assert_eq!(
        card_line(&waiting, Some(&[asked("Which runner?")])).as_deref(),
        Some("The agent asks: Which runner?")
    );
    assert_eq!(card_line(&task(TaskState::Blocked), Some(&[])), None);
    assert_eq!(
        card_line(&task(TaskState::Failed), Some(&[])).as_deref(),
        Some("The agent ended its turn. Reply in the terminal, or resume it.")
    );
}

#[test]
fn shows_the_step_only_with_two_steps_or_more() {
    assert_eq!(step_text(1, 1), None);
    assert_eq!(step_text(3, 5).as_deref(), Some("3/5"));
}

#[test]
fn keeps_a_new_title_only_when_it_changes() {
    assert_eq!(renamed_title("  New  ", "Old").as_deref(), Some("New"));
    assert_eq!(renamed_title("   ", "Old"), None);
    assert_eq!(renamed_title("Old ", "Old"), None);
}

#[test]
fn says_what_removing_a_project_keeps() {
    assert_eq!(
        remove_project_title("natamux"),
        "Remove natamux from Natamux?"
    );
    assert!(remove_project_description("/src/natamux").contains("/src/natamux"));
}

fn ids_of<C>(items: &[MenuItem<C>]) -> Vec<&'static str> {
    items
        .iter()
        .filter_map(|menu_item| match menu_item {
            MenuItem::Item { id, .. } => Some(*id),
            MenuItem::Separator => None,
        })
        .collect()
}

#[test]
fn every_row_and_project_menu_item_is_a_command_of_the_registry() {
    let running = task(TaskState::Running);
    let mut ids = ids_of(&row_menu(&running, false));
    ids.extend(ids_of(&row_menu(&running, true)));
    let full = ProjectMenuInput {
        finished: 2,
        ..project_input()
    };
    ids.extend(ids_of(&project_menu(full)));
    let blurred = ProjectMenuInput {
        blurred: true,
        ..full
    };
    ids.extend(ids_of(&project_menu(blurred)));
    for id in ids {
        assert!(
            crate::app::commands::command_by_id(id).is_some(),
            "{id} is not in the registry"
        );
    }
}
