use std::collections::BTreeMap;

use natamux_proto::tasks::task::TaskState;

use super::*;
use crate::palette::jump_rows::{JumpInput, jump_sections};
use crate::tasks::task_time::parse_iso_ms;

pub(super) fn now() -> i64 {
    parse_iso_ms("2026-10-09T12:00:00.000Z").unwrap_or(0)
}

pub(super) fn task(id: &str, state: TaskState) -> Task {
    Task {
        id: id.into(),
        title: format!("Task {id}"),
        state,
        workspace: Some(format!("/work/{id}")),
        stage: "implement".into(),
        created_at: "2026-10-09T08:00:00.000Z".into(),
        updated_at: "2026-10-09T11:55:00.000Z".into(),
        ..Task::default()
    }
}

pub(super) fn updated(mut task: Task, at: &str) -> Task {
    task.updated_at = at.into();
    task
}

pub(super) fn no_workspace(mut task: Task) -> Task {
    task.workspace = None;
    task
}

pub(super) fn command(id: &'static str, title: &str, suggested: Option<u8>) -> PaletteCommand {
    PaletteCommand {
        id,
        title: title.into(),
        key: None,
        suggested,
    }
}

// The commands that can run now with nothing open, in registry order.
pub(super) fn commands() -> Vec<PaletteCommand> {
    vec![
        command("workspace.new", "Task: Start a task", Some(3)),
        command(
            "help.shortcuts",
            "App: Show the keyboard shortcuts",
            Some(2),
        ),
        command("view.board", "View: Board", None),
        command("theme.system", "Theme: Follow the system", None),
        command("theme.light", "Theme: Light", None),
        command("theme.dark", "Theme: Dark", None),
        command("repo.add", "Project: Add a project", None),
        command("repo.import", "Project: Import GitHub issues", None),
    ]
}

pub(super) fn strings(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

pub(super) struct Base {
    pub(super) tasks: Vec<Task>,
    pub(super) commands: Vec<PaletteCommand>,
    pub(super) repos: Vec<RepoItem>,
    pub(super) recent: Vec<String>,
    pub(super) scope: Option<PaletteScope>,
}

pub(super) fn base() -> Base {
    Base {
        tasks: Vec::new(),
        commands: commands(),
        repos: Vec::new(),
        recent: Vec::new(),
        scope: None,
    }
}

impl Base {
    pub(super) fn all(&self, query: &str) -> Vec<PaletteSection> {
        all_sections(&AllInput {
            query,
            tasks: &self.tasks,
            commands: &self.commands,
            repos: &self.repos,
            recent: &self.recent,
            now_ms: now(),
            scope: self.scope.as_ref(),
        })
    }

    pub(super) fn jump(&self, query: &str) -> Vec<PaletteSection> {
        self.jump_with(query, &BTreeMap::new())
    }

    pub(super) fn jump_with(
        &self,
        query: &str,
        terminals: &BTreeMap<String, Vec<String>>,
    ) -> Vec<PaletteSection> {
        jump_sections(&JumpInput {
            query,
            tasks: &self.tasks,
            recent: &self.recent,
            now_ms: now(),
            scope: self.scope.as_ref(),
            terminals,
        })
    }
}

pub(super) fn label(row: &PaletteRow) -> String {
    match row {
        PaletteRow::Task { task, .. } => task.id.clone(),
        PaletteRow::Shell { task, name } => format!("{}/{name}", task.id),
        PaletteRow::Command { command, .. } => command.id.to_owned(),
        PaletteRow::Repo { repo, .. } => format!("repo:{}", repo.id),
        PaletteRow::CreateTask { .. } => "create-task".into(),
        PaletteRow::NewWorkspace { .. } => "new-workspace".into(),
    }
}

pub(super) fn ids(sections: &[PaletteSection]) -> Vec<String> {
    flat_rows(sections).iter().map(label).collect()
}

pub(super) fn titles(sections: &[PaletteSection]) -> Vec<Option<&'static str>> {
    sections.iter().map(|section| section.title).collect()
}

fn empty_tasks() -> Vec<Task> {
    vec![
        task("T-0001", TaskState::Running),
        updated(
            task("T-0002", TaskState::NeedsInput),
            "2026-10-09T10:00:00.000Z",
        ),
        updated(
            task("T-0003", TaskState::Review),
            "2026-10-09T09:00:00.000Z",
        ),
        task("T-0004", TaskState::Done),
        no_workspace(task("T-0005", TaskState::Draft)),
    ]
}

#[test]
fn shows_what_needs_the_user_first_longest_wait_first_then_recent_then_suggestions() {
    let mut input = base();
    input.tasks = empty_tasks();
    input.recent = strings(&["T-0004", "T-0001", "T-0005"]);
    let sections = input.all("");
    assert_eq!(
        titles(&sections),
        [Some("Needs you"), Some("Recent"), Some("Suggested")]
    );
    assert_eq!(ids(&sections[..1]), ["T-0003", "T-0002"]);
    // Only workspaces are recent, and a task that already shows under Needs you does not repeat.
    assert_eq!(ids(&sections[1..2]), ["T-0004", "T-0001"]);
    assert!(ids(&sections[2..]).contains(&"workspace.new".to_owned()));
}

#[test]
fn caps_needs_you_at_5_and_recent_at_5() {
    let many = (0..8).map(|index| {
        updated(
            task(&format!("T-01{index}"), TaskState::Review),
            &format!("2026-10-09T0{index}:00:00.000Z"),
        )
    });
    let more: Vec<Task> = (0..8)
        .map(|index| task(&format!("T-02{index}"), TaskState::Done))
        .collect();
    let mut input = base();
    input.recent = more.iter().map(|task| task.id.clone()).collect();
    input.tasks = many.chain(more).collect();
    let sections = input.all("");
    assert_eq!(sections[0].rows.len(), 5);
    assert_eq!(sections[1].rows.len(), 5);
}

// The web lists them in its registry order; the app's registry has another order, so a rank.
#[test]
fn lists_the_suggested_commands_by_their_rank() {
    let sections = base().all("");
    let suggested = sections
        .iter()
        .find(|section| section.id == "suggested")
        .map(|section| ids(std::slice::from_ref(section)))
        .unwrap_or_default();
    assert_eq!(suggested, ["help.shortcuts", "workspace.new"]);
}

// The caller passes only the commands that can run now: the rule is that nothing else is listed.
#[test]
fn only_offers_commands_that_can_run_now() {
    let suggested = ids(&base().all(""));
    assert!(suggested.contains(&"help.shortcuts".to_owned()));
    assert!(!suggested.contains(&"tab.terminal".to_owned()));
}

#[test]
fn narrows_to_what_needs_the_user_with_that_scope_and_drops_recent_and_suggested() {
    let mut input = base();
    input.tasks = empty_tasks();
    input.scope = Some(PaletteScope::NeedsYou);
    input.recent = strings(&["T-0004"]);
    assert_eq!(titles(&input.all("")), [Some("Needs you")]);
}

fn query_tasks() -> Vec<Task> {
    let mut first = task("T-0007", TaskState::Running);
    first.title = "Fix token refresh race".into();
    first.branch = Some("natamux/t-0007-fix".into());
    let mut second = task("T-0008", TaskState::Ready);
    second.title = "Write docs".into();
    vec![first, second]
}

#[test]
fn finds_tasks_by_title_id_and_branch_and_highlights_the_title() {
    let mut input = base();
    input.tasks = query_tasks();
    let by_title = input.all("token");
    match &by_title[0].rows[0] {
        PaletteRow::Task {
            task, title_ranges, ..
        } => {
            assert_eq!(task.id, "T-0007");
            assert!(!title_ranges.is_empty());
        }
        other => panic!("not a task row: {other:?}"),
    }
    assert!(ids(&input.all("T-0008")).contains(&"T-0008".to_owned()));
    assert_eq!(ids(&input.all("t-0007-fix")), ["T-0007"]);
}

#[test]
fn finds_commands_with_their_category_in_front() {
    let mut input = base();
    input.tasks = query_tasks();
    let sections = input.all("theme dark");
    assert_eq!(sections[0].title, Some("Commands"));
    assert_eq!(ids(&sections)[0], "theme.dark");
}

#[test]
fn searches_commands_only_after_the_greater_than_sign() {
    let mut input = base();
    input.tasks = query_tasks();
    assert!(input.all("> zzq").is_empty());
    let all = input.all(">");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, Some("Commands"));
    assert!(ids(&all).contains(&"theme.light".to_owned()));
    assert!(!ids(&all).contains(&"T-0007".to_owned()));
    assert_eq!(ids(&input.all(">board"))[0], "view.board");
}

#[test]
fn keeps_the_registry_order_for_a_greater_than_sign_with_nothing_after_it() {
    let input = base();
    let want: Vec<String> = commands().iter().map(|c| c.id.to_owned()).collect();
    assert_eq!(ids(&input.all(">")), want);
}

#[test]
fn searches_tasks_by_id_only_after_the_hash_sign() {
    let mut input = base();
    input.tasks = query_tasks();
    assert_eq!(ids(&input.all("#7")), ["T-0007"]);
    assert_eq!(ids(&input.all("#t-000")), ["T-0008", "T-0007"]);
    assert!(input.all("#99").is_empty());
}

#[test]
fn offers_to_create_a_task_or_a_workspace_when_nothing_matches() {
    let mut input = base();
    input.tasks = query_tasks();
    let rows = flat_rows(&input.all("zzzqx"));
    assert_eq!(ids(&input.all("zzzqx")), ["create-task", "new-workspace"]);
    assert_eq!(
        rows[0],
        PaletteRow::CreateTask {
            title: "zzzqx".into()
        }
    );
}

#[test]
fn starts_a_task_with_the_query_as_its_title_without_the_spaces_around_it() {
    let input = base();
    let rows = flat_rows(&input.all("  zzzqx \"a b\"  "));
    assert_eq!(
        rows[0].create_label().as_deref(),
        Some("Add task “zzzqx \"a b\"”")
    );
    assert_eq!(
        rows[1].create_label().as_deref(),
        Some("Start task “zzzqx \"a b\"”")
    );
    assert_eq!(
        rows[1],
        PaletteRow::NewWorkspace {
            title: "zzzqx \"a b\"".into()
        }
    );
}

#[test]
fn offers_no_add_task_or_start_task_rows_while_nothing_is_typed() {
    let mut input = base();
    input.tasks = empty_tasks();
    let rows = flat_rows(&input.all(""));
    assert!(rows.iter().all(|row| row.create_label().is_none()));
}

#[test]
fn lists_the_project_commands_under_project_not_repo() {
    let input = base();
    let titles: Vec<String> = flat_rows(&input.all(">project"))
        .into_iter()
        .filter_map(|row| match row {
            PaletteRow::Command { command, .. } => Some(command.title),
            _ => None,
        })
        .collect();
    assert!(titles.iter().any(|t| t == "Project: Add a project"));
    assert!(titles.iter().any(|t| t == "Project: Import GitHub issues"));
    assert!(!titles.iter().any(|t| t.starts_with("Repo:")));
}

#[test]
fn lists_the_start_a_task_command_under_task() {
    let input = base();
    let first = flat_rows(&input.all(">start a task"));
    assert!(matches!(
        &first[0],
        PaletteRow::Command { command, .. } if command.title == "Task: Start a task"
    ));
}

#[test]
fn finds_a_repository_and_offers_a_workspace_in_it() {
    let mut input = base();
    input.tasks = query_tasks();
    input.repos = vec![RepoItem {
        id: "qw".into(),
        name: "qwertzuiop".into(),
    }];
    assert_eq!(ids(&input.all("qwertz")), ["repo:qw"]);
}

#[test]
fn finds_a_project_under_projects() {
    let mut input = base();
    input.repos = vec![RepoItem {
        id: "web".into(),
        name: "Web app".into(),
    }];
    let sections = input.all("web app");
    assert!(sections.iter().any(|s| s.title == Some("Projects")));
    assert_eq!(ids(&sections)[0], "repo:web");
}

fn section(id: &'static str, count: usize) -> PaletteSection {
    PaletteSection {
        id,
        title: Some(id),
        rows: (0..count)
            .map(|index| PaletteRow::CreateTask {
                title: format!("{id}{index}"),
            })
            .collect(),
    }
}

#[test]
fn keeps_the_first_rows_and_drops_the_sections_after_the_cap() {
    let capped = cap_rows(vec![section("a", 30), section("b", 30), section("c", 5)]);
    let counts: Vec<usize> = capped.iter().map(|s| s.rows.len()).collect();
    assert_eq!(counts, [30, MAX_ROWS - 30]);
}

#[test]
fn returns_short_lists_as_they_are() {
    let sections = vec![section("a", 3)];
    assert_eq!(cap_rows(sections.clone()), sections);
}

#[test]
fn cap_rows_with_nothing_returns_no_sections() {
    assert!(cap_rows(Vec::new()).is_empty());
}

#[test]
fn has_five_empty_messages() {
    let all = PaletteMode::All;
    assert_eq!(empty_text(all, None, " > zz", ""), "No command matches.");
    assert_eq!(
        empty_text(PaletteMode::Jump, None, "", ""),
        "No workspace yet."
    );
    assert_eq!(
        empty_text(all, None, "", ""),
        "Nothing to show yet. Type to search."
    );
}

#[test]
fn adds_a_task_to_the_agents_grid_until_it_holds_six_panes() {
    let two = strings(&["T-1", "T-2"]);
    assert_eq!(
        add_to_agents(&two, "T-3"),
        Ok(strings(&["T-1", "T-2", "T-3"]))
    );
    assert_eq!(add_to_agents(&two, "T-2"), Ok(two.clone()));
    let full = strings(&["T-1", "T-2", "T-3", "T-4", "T-5", "T-6"]);
    assert_eq!(
        add_to_agents(&full, "T-7"),
        Err("The Agents grid holds 6 panes. Close one to add T-7.".to_owned())
    );
    assert_eq!(add_to_agents(&full, "T-6"), Ok(full.clone()));
}

#[test]
fn keys_its_rows_as_the_web_does() {
    let t = task("T-0001", TaskState::Running);
    let rows = [
        (
            PaletteRow::Task {
                task: t.clone(),
                title_ranges: Vec::new(),
                id_ranges: Vec::new(),
            },
            "task:T-0001",
        ),
        (
            PaletteRow::Shell {
                task: t,
                name: "2".into(),
            },
            "shell:T-0001:2",
        ),
        (PaletteRow::CreateTask { title: "x".into() }, "create-task"),
        (
            PaletteRow::NewWorkspace { title: "x".into() },
            "new-workspace",
        ),
    ];
    for (row, key) in rows {
        assert_eq!(row.key(), key);
    }
}
