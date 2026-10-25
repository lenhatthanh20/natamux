use serde_json::json;

use super::*;
use crate::tasks::task_rules::actions_for;
use crate::tasks::task_time::parse_iso_ms;
use crate::workspaces::branches::branch_options;
use natamux_proto::tasks::task::PullRequestStatus;

fn task(state: TaskState) -> Task {
    Task {
        id: "T-0001".into(),
        title: "Fix".into(),
        state,
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn list() -> BranchList {
    BranchList {
        branches: ["main", "natamux/t-0001", "other"]
            .map(String::from)
            .to_vec(),
        current: Some("other".into()),
        default_branch: Some("main".into()),
    }
}

/// The text of the "Start from branch" box: the label of the start branch's option.
fn start_label(parent: Option<&str>) -> String {
    let list = list();
    let start = details_start_branch(parent, Some(&list));
    let options = branch_options(
        &list.branches,
        list.current.as_deref(),
        None,
        start.as_deref(),
    );
    options[0].label.clone()
}

#[test]
fn starts_from_the_branch_of_the_parent_when_it_is_in_the_list() {
    assert_eq!(
        start_label(Some("natamux/t-0001")),
        "natamux/t-0001 (default)"
    );
}

#[test]
fn starts_from_the_default_branch_when_the_parent_branch_is_not_in_the_list() {
    assert_eq!(start_label(Some("natamux/gone")), "main (default)");
    assert_eq!(start_label(None), "main (default)");
}

#[test]
fn says_a_ready_parent_waits_for_its_open_subtask() {
    assert_eq!(
        waits_for_text(&["T-0002".to_string()]).as_deref(),
        Some("Waits for T-0002 to be done")
    );
    assert_eq!(
        waits_for_text(&["T-2".into(), "T-3".into()]).as_deref(),
        Some("Waits for T-2, T-3 to be done")
    );
    assert_eq!(waits_for_text(&[]), None);
}

#[test]
fn says_how_long_the_agent_was_quiet() {
    let now = parse_iso_ms("2026-10-08T09:12:00.000Z").unwrap();
    assert_eq!(
        quiet_banner("2026-10-08T09:00:00.000Z", now, 0),
        "No sign of life for 12m. The agent may be stuck."
    );
}

#[test]
fn labels_the_agent_button_by_what_it_does() {
    let start = agent_button(AgentAction::Start, &[], false);
    assert_eq!((start.label, start.icon), ("Start agent", "play"));
    assert!(start.tip.starts_with("Creates the workspace"));
    let restart = agent_button(AgentAction::Restart, &[], false);
    assert_eq!(
        (restart.label, restart.icon),
        ("Restart agent", "rotate-ccw")
    );
    assert_eq!(
        restart.tip,
        "Starts Claude Code again in the terminal of the workspace"
    );
    assert!(!restart.disabled);
}

#[test]
fn disables_the_agent_button_while_busy_or_waiting_for_other_tasks() {
    assert!(agent_button(AgentAction::Start, &[], true).disabled);
    let waiting = agent_button(AgentAction::Start, &["T-2".to_string()], false);
    assert!(waiting.disabled);
    assert_eq!(waiting.tip, "Waits for T-2 to be done");
}

#[test]
fn offers_more_tries_only_for_a_task_that_is_not_finished_and_has_an_agent_button() {
    assert!(can_fan_out(TaskState::Ready, Some(AgentAction::Start)));
    assert!(!can_fan_out(TaskState::Running, None));
    assert!(!can_fan_out(TaskState::Done, Some(AgentAction::Start)));
}

#[test]
fn lists_the_items_of_the_more_menu_with_a_line_before_the_danger_ones() {
    let actions = actions_for(TaskState::Ready);
    let cancel = actions.iter().find(|action| action.confirm);
    let menu = more_menu(false, true, true, cancel, true);
    assert_eq!(
        menu_labels(&menu),
        [
            "Save as template",
            "Run more tries...",
            "---",
            "Cancel task",
            "Delete task"
        ]
    );
    assert!(more_menu(false, false, false, None, false).is_empty());
    assert_eq!(more_menu(false, false, false, None, true).len(), 1);
    assert_eq!(more_menu(false, true, false, None, false).len(), 1);
}

/// T-0171: the pencil left the dock header, so Edit task leads the menu while no form is open.
#[test]
fn puts_edit_task_first_with_a_line_after_it() {
    let menu = more_menu(true, true, false, None, true);
    assert_eq!(
        menu_labels(&menu),
        ["Edit task", "---", "Save as template", "---", "Delete task"]
    );
    assert_eq!(
        menu_labels(&more_menu(true, false, false, None, false)),
        ["Edit task"]
    );
    assert!(!menu_labels(&more_menu(false, true, false, None, true)).contains(&"Edit task"));
}

fn menu_labels(menu: &[MoreItem]) -> Vec<&str> {
    menu.iter()
        .map(|entry| match entry {
            MoreItem::Item { label, .. } => label.as_str(),
            MoreItem::Separator => "---",
        })
        .collect()
}

#[test]
fn lists_the_fields_of_the_details_section() {
    let now = parse_iso_ms("2026-10-08T09:05:00.000Z").unwrap();
    let plain = details_fields(&task(TaskState::Ready), now, 0);
    let labels: Vec<&str> = plain.iter().map(|field| field.label).collect();
    assert_eq!(labels, ["Depends on", "Created", "Updated"]);
    assert_eq!(plain[0].text, "-");
    assert_eq!(plain[1].text, "5m ago");
    assert_eq!(plain[1].tip.as_deref(), Some("Oct 8, 2026, 9:00:00 AM"));
    let full = Task {
        repo: Some("natamux".into()),
        depends_on: vec!["T-2".into(), "T-3".into()],
        parent_id: Some("T-9".into()),
        ..task(TaskState::Ready)
    };
    let fields = details_fields(&full, now, 0);
    let labels: Vec<&str> = fields.iter().map(|field| field.label).collect();
    assert_eq!(
        labels,
        ["Project", "Depends on", "Part of", "Created", "Updated"]
    );
    assert_eq!(fields[1].text, "T-2, T-3");
}

#[test]
fn gives_each_task_its_ports() {
    let table = [
        ("T-1", 20_010, 20_019),
        ("T-0042", 20_420, 20_429),
        ("T-1001", 20_010, 20_019),
        ("T-0", 20_000, 20_009),
        ("TASK-0005", 20_050, 20_059),
        ("T-0005", 20_050, 20_059),
        ("TASK-1005", 20_050, 20_059),
        ("ABC-0", 20_000, 20_009),
        ("ABC-12", 20_120, 20_129),
        ("XYZ-34", 20_340, 20_349),
    ];
    for (id, first, last) in table {
        assert_eq!(port_range(id), (first, last), "{id}");
    }
    assert_eq!(ports_text("T-0042"), "20420–20429");
}

#[test]
fn says_the_state_and_the_checks() {
    let mut one = task(TaskState::Review);
    assert_eq!(pull_request_summary(&one), None);
    one.pr_status = Some(PullRequestStatus {
        state: "open".into(),
        checks: "failing".into(),
    });
    assert_eq!(
        pull_request_summary(&one).as_deref(),
        Some("open, checks fail")
    );
    one.pr_status = Some(PullRequestStatus {
        state: "merged".into(),
        checks: "none".into(),
    });
    assert_eq!(pull_request_summary(&one).as_deref(), Some("merged"));
    one.pr_status = Some(PullRequestStatus {
        state: "draft".into(),
        checks: "pending".into(),
    });
    assert_eq!(
        pull_request_summary(&one).as_deref(),
        Some("draft, checks running")
    );
}

#[test]
fn colours_the_checks_and_shortens_the_address() {
    assert_eq!(checks_tone("passing"), ChecksTone::Add);
    assert_eq!(checks_tone("failing"), ChecksTone::Remove);
    assert_eq!(checks_tone("pending"), ChecksTone::Signal);
    assert_eq!(checks_tone("none"), ChecksTone::Muted);
    assert_eq!(
        pull_request_label("https://github.com/o/r/pull/12"),
        "o/r/pull/12"
    );
    assert_eq!(
        pull_request_label("https://example.com/x"),
        "https://example.com/x"
    );
}

#[test]
fn counts_the_done_subtasks() {
    assert_eq!(subtasks_aside(&[]), None);
    let list = [
        task(TaskState::Done),
        task(TaskState::Running),
        task(TaskState::Done),
    ];
    assert_eq!(subtasks_aside(&list).as_deref(), Some("2 of 3 done"));
}

#[test]
fn answers_in_the_terminal_whose_agent_asks() {
    let mut asking = task(TaskState::NeedsInput);
    asking.stage = "spec".into();
    let list: TerminalList = serde_json::from_value(json!({
        "terminals": ["1", "5", "8"],
        "lanes": {
            "5": {"stage": "spec", "state": "running"},
            "8": {"stage": "review", "state": "needs-input"}
        }
    }))
    .unwrap();
    assert_eq!(asking_terminal(Some(&list), &asking), "8");
    let quiet: TerminalList =
        serde_json::from_value(json!({"lanes": {"5": {"stage": "spec"}}})).unwrap();
    assert_eq!(asking_terminal(Some(&quiet), &asking), "5");
    assert_eq!(asking_terminal(None, &asking), "1");
}

#[test]
fn the_panel_waits_for_the_soonest_time_it_shows() {
    let now = parse_iso_ms("2026-10-08T09:30:20.000Z").unwrap();
    // The task's ages change at 09:31:00 and its figures at 09:30:30.
    let open = task(TaskState::Review);
    assert_eq!(details_wait(&open, [], &[], now), Some(10_000));
    // An event of 09:30:15 shows a new age at 09:31:15; the figures still come first.
    let event = "2026-10-08T09:30:15.000Z";
    assert_eq!(details_wait(&open, [event], &[], now), Some(10_000));
    // A done task shows no running figures: its next change is the age at 09:31:00.
    let done = task(TaskState::Done);
    assert_eq!(details_wait(&done, [event], &[], now), Some(40_000));
    assert_eq!(details_wait(&done, [], &[], now), Some(40_000));
}

