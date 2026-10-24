use std::collections::BTreeMap;

use natamux_proto::tasks::task::{Task, TaskState};

use super::tests::*;
use super::*;
use crate::palette::jump_rows::terminal_reads;

fn jump_tasks() -> Vec<Task> {
    vec![
        task("T-0001", TaskState::Ready),
        task("T-0002", TaskState::Running),
        task("T-0003", TaskState::Failed),
        task("T-0004", TaskState::Review),
        task("T-0005", TaskState::NeedsInput),
        task("T-0006", TaskState::Blocked),
        task("T-0007", TaskState::Done),
        no_workspace(task("T-0008", TaskState::Draft)),
        updated(
            task("T-0009", TaskState::Running),
            "2026-10-09T10:00:00.000Z",
        ),
    ]
}

fn jump_base() -> Base {
    let mut input = base();
    input.tasks = jump_tasks();
    input
}

#[test]
fn lists_workspaces_needs_you_first_by_how_much_they_need_you_then_running_then_recent() {
    let mut input = jump_base();
    input.recent = strings(&["T-0007", "T-0001"]);
    let sections = input.jump("");
    assert_eq!(
        titles(&sections),
        [Some("Needs you"), Some("Running"), Some("Recent")]
    );
    // T-0009 is running but quiet: it counts as needing the user, after the others.
    assert_eq!(
        ids(&sections[..1]),
        ["T-0005", "T-0004", "T-0003", "T-0006", "T-0009"]
    );
    assert_eq!(ids(&sections[1..2]), ["T-0002"]);
    assert_eq!(ids(&sections[2..3]), ["T-0007", "T-0001"]);
}

#[test]
fn leaves_out_tasks_without_a_workspace() {
    assert!(!ids(&jump_base().jump("")).contains(&"T-0008".to_owned()));
}

#[test]
fn gives_the_same_order_for_the_same_tasks_however_the_list_arrives() {
    let first = ids(&jump_base().jump(""));
    let mut reversed = jump_base();
    reversed.tasks.reverse();
    assert_eq!(ids(&reversed.jump("")), first);
}

#[test]
fn shows_the_other_terminals_of_a_workspace_after_it() {
    let mut input = base();
    input.tasks = vec![task("T-0001", TaskState::Running)];
    let terminals = BTreeMap::from([("T-0001".to_owned(), strings(&["1", "2", "3"]))]);
    assert_eq!(
        ids(&input.jump_with("", &terminals)),
        ["T-0001", "T-0001/2", "T-0001/3"]
    );
}

#[test]
fn narrows_to_the_tasks_that_need_the_user() {
    let mut input = jump_base();
    input.scope = Some(PaletteScope::NeedsYou);
    assert_eq!(
        ids(&input.jump("")),
        ["T-0005", "T-0004", "T-0003", "T-0006", "T-0009"]
    );
}

#[test]
fn lists_a_task_that_needs_the_user_and_has_no_workspace_but_only_in_the_needs_you_scope() {
    let mut input = jump_base();
    input
        .tasks
        .push(no_workspace(task("T-0010", TaskState::Blocked)));
    input.scope = Some(PaletteScope::NeedsYou);
    let scoped = ids(&input.jump(""));
    assert!(scoped.contains(&"T-0010".to_owned()));
    assert!(!scoped.contains(&"T-0008".to_owned()));
    input.scope = None;
    assert!(!ids(&input.jump("")).contains(&"T-0010".to_owned()));
}

#[test]
fn sorts_the_needs_you_scope_by_urgency_not_by_the_longest_wait_with_or_without_a_workspace() {
    let mut input = base();
    input.tasks = vec![
        updated(
            no_workspace(task("T-0011", TaskState::Blocked)),
            "2026-10-09T08:00:00.000Z",
        ),
        updated(
            no_workspace(task("T-0012", TaskState::Review)),
            "2026-10-09T09:00:00.000Z",
        ),
        updated(
            task("T-0013", TaskState::NeedsInput),
            "2026-10-09T11:00:00.000Z",
        ),
    ];
    input.scope = Some(PaletteScope::NeedsYou);
    assert_eq!(ids(&input.jump("")), ["T-0013", "T-0012", "T-0011"]);
}

#[test]
fn finds_a_waiting_task_without_a_workspace_by_its_title_in_the_needs_you_scope() {
    let mut waiting = no_workspace(task("T-0010", TaskState::Blocked));
    waiting.title = "Ship docs".into();
    let mut input = base();
    input.tasks = vec![waiting];
    input.scope = Some(PaletteScope::NeedsYou);
    assert_eq!(ids(&input.jump("ship")), ["T-0010"]);
}

#[test]
fn lists_nothing_for_no_tasks_with_or_without_a_scope() {
    let mut input = base();
    assert!(input.jump("").is_empty());
    input.scope = Some(PaletteScope::NeedsYou);
    assert!(input.jump("").is_empty());
}

#[test]
fn searches_workspaces_and_offers_a_new_one_when_nothing_matches() {
    let input = jump_base();
    assert_eq!(ids(&input.jump("T-0003")), ["T-0003"]);
    assert_eq!(ids(&input.jump("qqqq")), ["new-workspace"]);
}

#[test]
fn numbers_the_first_nine_rows() {
    assert_eq!(digit_of(0), Some(1));
    assert_eq!(digit_of(8), Some(9));
    assert_eq!(digit_of(9), None);
}

fn scope_tasks() -> Vec<Task> {
    let with_repo = |id: &str, state, repo: &str| {
        let mut made = task(id, state);
        made.repo = Some(repo.into());
        made
    };
    vec![
        with_repo("T-0001", TaskState::Running, "r1"),
        with_repo("T-0002", TaskState::NeedsInput, "r1"),
        with_repo("T-0003", TaskState::NeedsInput, "r2"),
        task("T-0004", TaskState::Done),
    ]
}

fn scope_base(repo: &str) -> Base {
    let mut input = base();
    input.tasks = scope_tasks();
    input.scope = Some(PaletteScope::Repo(repo.into()));
    input
}

#[test]
fn lists_only_the_workspaces_of_that_project_in_the_jump_palette() {
    let mut found = ids(&scope_base("r1").jump(""));
    found.sort();
    assert_eq!(found, ["T-0001", "T-0002"]);
    // A task with no repo belongs to the default project.
    assert_eq!(ids(&scope_base("default").jump("")), ["T-0004"]);
}

#[test]
fn narrows_a_typed_query_inside_the_project() {
    assert_eq!(ids(&scope_base("r1").jump("T-0002")), ["T-0002"]);
    assert_eq!(ids(&scope_base("r1").jump("T-0003")), ["new-workspace"]);
}

#[test]
fn keeps_only_the_waiting_tasks_of_the_project_when_nothing_is_typed() {
    let mut input = scope_base("r1");
    input.recent = strings(&["T-0001"]);
    let sections = input.all("");
    assert_eq!(titles(&sections), [Some("Needs you")]);
    assert_eq!(ids(&sections), ["T-0002"]);

    let mut found: Vec<String> = flat_rows(&input.all("Task"))
        .iter()
        .filter(|row| matches!(row, PaletteRow::Task { .. }))
        .map(label)
        .collect();
    found.sort();
    assert_eq!(found, ["T-0001", "T-0002"]);
}

#[test]
fn cycles_the_scope_with_tab_needs_you_then_each_project_that_has_a_task() {
    let mut input = base();
    input.tasks = vec![
        {
            let mut made = task("T-0001", TaskState::Running);
            made.repo = Some("a".into());
            made
        },
        {
            let mut made = task("T-0002", TaskState::Running);
            made.repo = Some("b".into());
            made
        },
    ];
    let repos: Vec<RepoItem> = [("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")]
        .iter()
        .map(|(id, name)| RepoItem {
            id: (*id).into(),
            name: (*name).into(),
        })
        .collect();
    let projects = repos_with_tasks(&repos, &input.tasks);
    assert_eq!(projects.len(), 2);

    let mut scope = None;
    let mut names = Vec::new();
    for _ in 0..3 {
        scope = next_scope(scope.as_ref(), &projects);
        names.push(scope.as_ref().map(|each| scope_name(each, &repos)));
    }
    assert_eq!(
        names,
        [
            Some("Needs you".to_owned()),
            Some("Alpha".to_owned()),
            Some("Beta".to_owned())
        ]
    );
    input.scope = scope.clone();
    assert_eq!(ids(&input.jump("")), ["T-0002"]);
    assert_eq!(next_scope(scope.as_ref(), &projects), None);
}

#[test]
fn says_when_a_project_has_no_workspace() {
    let text = empty_text(
        PaletteMode::Jump,
        Some(&PaletteScope::Repo("a".into())),
        "",
        "Alpha",
    );
    assert_eq!(text, "No workspace in Alpha.");
}

#[test]
fn says_nothing_needs_you_when_only_a_finished_task_has_no_workspace() {
    let mut input = base();
    input.tasks = vec![no_workspace(task("T-0001", TaskState::Done))];
    input.scope = Some(PaletteScope::NeedsYou);
    assert!(input.jump("").is_empty());
    assert_eq!(
        empty_text(PaletteMode::Jump, input.scope.as_ref(), "", ""),
        "Nothing needs you."
    );
}

// `CommandPalette.tsx` reads the shells of each workspace while the jump palette is open, only
// when there are at most 20 workspaces.
#[test]
fn the_jump_palette_reads_the_terminals_of_every_workspace_up_to_20() {
    let tasks = jump_tasks();
    let reads = terminal_reads(PaletteMode::Jump, &tasks);
    assert_eq!(reads.len(), 8);
    assert!(!reads.contains(&"T-0008".to_owned()));
}

#[test]
fn the_jump_palette_reads_no_terminals_above_20_workspaces() {
    let tasks: Vec<Task> = (1..=21)
        .map(|n| task(&format!("T-{n:04}"), TaskState::Running))
        .collect();
    assert!(terminal_reads(PaletteMode::Jump, &tasks).is_empty());
    assert_eq!(terminal_reads(PaletteMode::Jump, &tasks[..20]).len(), 20);
}

#[test]
fn the_command_palette_reads_no_terminals() {
    assert!(terminal_reads(PaletteMode::All, &jump_tasks()).is_empty());
}
