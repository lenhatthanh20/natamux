use natamux_proto::terminals::list::TerminalLane;

use super::*;
use crate::tasks::task_time::parse_iso_ms;

fn task(id: &str, state: TaskState) -> Task {
    Task {
        id: id.into(),
        title: format!("Title of {id}"),
        state,
        repo: Some("a".into()),
        branch: Some(format!("natamux/{}", id.to_lowercase())),
        workspace: Some(format!("/work/{id}")),
        updated_at: "2026-10-22T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn repo(id: &str) -> Repo {
    Repo {
        id: id.into(),
        name: format!("project {id}"),
        path: format!("/src/{id}"),
        worktrees_dir: None,
    }
}

struct Fixture {
    repos: Vec<Repo>,
    tasks: Vec<Task>,
    filter: String,
    current: Option<String>,
    pinned: Vec<String>,
    folds: Vec<(String, bool)>,
    terminals: BTreeMap<String, TerminalList>,
}

impl Fixture {
    fn new(tasks: Vec<Task>) -> Self {
        Fixture {
            repos: vec![repo("a")],
            tasks,
            filter: String::new(),
            current: None,
            pinned: Vec::new(),
            folds: Vec::new(),
            terminals: BTreeMap::new(),
        }
    }

    fn model(&self) -> TreeModel {
        TreeModel::build(&TreeInput {
            repos: &self.repos,
            tasks: &self.tasks,
            filter: &self.filter,
            current_id: self.current.as_deref(),
            held_id: None,
            pinned: &self.pinned,
            folds: &self.folds,
            terminals: &self.terminals,
        })
    }
}

fn keys(model: &TreeModel) -> Vec<String> {
    model
        .lines
        .iter()
        .filter_map(|line| line.key(&model.groups.groups))
        .collect()
}

fn terminals(names: &[&str]) -> TerminalList {
    TerminalList {
        terminals: names.iter().map(|name| (*name).to_owned()).collect(),
        lanes: names
            .iter()
            .map(|name| {
                let lane = TerminalLane {
                    stage: Some("implement".into()),
                    state: None,
                    agent: Some(Some("claude-code".into())),
                };
                ((*name).to_owned(), lane)
            })
            .collect(),
        ..TerminalList::default()
    }
}

#[test]
fn the_lines_that_take_focus_are_the_keyboard_entries() {
    let mut parent = task("T-0002", TaskState::Running);
    parent.title = "Parent".into();
    let mut child = task("T-0003", TaskState::NeedsInput);
    child.parent_id = Some("T-0002".into());
    let mut fixture = Fixture::new(vec![
        parent,
        child,
        task("T-0004", TaskState::Done),
        task("T-0005", TaskState::Ready),
    ]);
    fixture.pinned = vec!["T-0005".into()];
    fixture.folds = vec![("finished:a".into(), true)];
    let model = fixture.model();
    let entry_keys: Vec<String> = model.entries.iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys(&model), entry_keys);
    assert_eq!(
        entry_keys,
        [
            "task:T-0005",
            "repo:a",
            "task:T-0002",
            "task:T-0003",
            "finished:a",
            "task:T-0004"
        ]
    );
}

#[test]
fn a_folded_group_shows_only_its_header() {
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Running)]);
    fixture.folds = vec![("repo:a".into(), false)];
    let model = fixture.model();
    assert_eq!(model.lines, [Line::GroupHeader { group: 0 }]);
    assert!(model.reads_terminals.is_empty());
}

#[test]
fn every_line_of_a_group_knows_its_group() {
    let model = Fixture::new(vec![task("T-0001", TaskState::Running)]).model();
    assert!(model.lines.iter().all(|line| line.group() == Some(0)));
}

#[test]
fn reads_the_terminals_of_working_waiting_and_open_tasks_only() {
    let mut fixture = Fixture::new(vec![
        task("T-0001", TaskState::Running),
        task("T-0002", TaskState::NeedsInput),
        task("T-0003", TaskState::Review),
        task("T-0004", TaskState::Review),
        task("T-0005", TaskState::Done),
    ]);
    fixture.current = Some("T-0004".into());
    fixture.folds = vec![("finished:a".into(), true)];
    let mut model = fixture.model();
    model.reads_terminals.sort();
    assert_eq!(model.reads_terminals, ["T-0001", "T-0002", "T-0004"]);
}

fn look_of<'a>(model: &'a TreeModel, id: &str) -> &'a TaskLook {
    model
        .lines
        .iter()
        .find_map(|line| match line {
            Line::Task { task, look, .. } if task.id == id => Some(look),
            _ => None,
        })
        .unwrap()
}

#[test]
fn only_the_open_card_carries_its_terminal_rows() {
    let mut fixture = Fixture::new(vec![
        task("T-0001", TaskState::Running),
        task("T-0002", TaskState::Running),
    ]);
    fixture.current = Some("T-0001".into());
    fixture
        .terminals
        .insert("T-0001".into(), terminals(&["1", "2"]));
    fixture.terminals.insert("T-0002".into(), terminals(&["1"]));
    let model = fixture.model();
    assert!(model.is_open("T-0001"));
    assert!(!model.is_open("T-0002"));
    let TaskLook::Open { rows, children } = look_of(&model, "T-0001") else {
        panic!("the current card is open");
    };
    let row_keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(row_keys, ["shell:1", "shell:2"]);
    assert!(children.is_empty());
    assert_eq!(look_of(&model, "T-0002"), &TaskLook::Closed);
    // The rows are entries for the keyboard, not lines. Same creation time: the higher id first.
    assert_eq!(keys(&model), ["repo:a", "task:T-0002", "task:T-0001"]);
    assert!(
        model
            .entries
            .iter()
            .any(|e| e.key == "agent:T-0001:shell:2")
    );
}

#[test]
fn every_entry_has_a_line_and_every_line_key_is_an_entry() {
    let mut child = task("T-0002", TaskState::Ready);
    child.parent_id = Some("T-0001".into());
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Running), child]);
    fixture.current = Some("T-0001".into());
    fixture.terminals.insert("T-0001".into(), terminals(&["1"]));
    let model = fixture.model();
    let entry_keys: Vec<&str> = model.entries.iter().map(|e| e.key.as_str()).collect();
    assert!(
        keys(&model)
            .iter()
            .all(|key| entry_keys.contains(&key.as_str()))
    );
    for entry in &model.entries {
        assert!(model.line_of(&entry.key).is_some(), "{}", entry.key);
    }
    let card = model.line_of("task:T-0001");
    assert_eq!(model.line_of("agent:T-0001:shell:1"), card);
    assert_eq!(model.line_of("task:T-0002"), card);
    assert_eq!(model.line_of("task:nothing"), None);
}

#[test]
fn a_saved_fold_opens_a_card_that_is_not_the_open_workspace() {
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Review)]);
    fixture.terminals.insert("T-0001".into(), terminals(&["1"]));
    assert!(!fixture.model().is_open("T-0001"));
    assert!(fixture.model().reads_terminals.is_empty());
    fixture.folds = vec![("task:T-0001".into(), true)];
    let model = fixture.model();
    assert!(model.is_open("T-0001"));
    assert_eq!(model.reads_terminals, ["T-0001"]);
    let TaskLook::Open { rows, .. } = look_of(&model, "T-0001") else {
        panic!("the card is open");
    };
    assert_eq!(rows.len(), 1);
}

#[test]
fn the_open_workspace_opens_without_a_saved_fold_and_a_saved_false_folds_it() {
    let mut waiting = task("T-0002", TaskState::NeedsInput);
    waiting.parent_id = Some("T-0001".into());
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Review), waiting]);
    fixture.current = Some("T-0001".into());
    fixture.terminals.insert("T-0001".into(), terminals(&["1"]));
    assert!(fixture.model().is_open("T-0001"));
    fixture.folds = vec![("task:T-0001".into(), false)];
    let model = fixture.model();
    assert!(!model.is_open("T-0001"));
    assert_eq!(look_of(&model, "T-0001"), &TaskLook::Closed);
    // The waiting subtask stays a nested line under the folded card.
    let nested = model
        .lines
        .iter()
        .any(|line| matches!(line, Line::Task { task, nested: true, .. } if task.id == "T-0002"));
    assert!(nested);
}

#[test]
fn an_open_card_carries_its_subtasks_and_a_finished_row_is_finished() {
    let mut child = task("T-0002", TaskState::Ready);
    child.parent_id = Some("T-0001".into());
    let mut fixture = Fixture::new(vec![
        task("T-0001", TaskState::Running),
        child,
        task("T-0003", TaskState::Done),
    ]);
    fixture.current = Some("T-0001".into());
    fixture.folds = vec![("finished:a".into(), true)];
    let model = fixture.model();
    let TaskLook::Open { children, .. } = look_of(&model, "T-0001") else {
        panic!("the current card is open");
    };
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "T-0002");
    assert_eq!(look_of(&model, "T-0003"), &TaskLook::Finished);
    assert!(!model.is_open("T-0003"));
}

#[test]
fn the_clock_also_follows_the_subtasks_inside_an_open_card() {
    let now = parse_iso_ms("2026-10-22T09:05:00.000Z").unwrap_or_default();
    let mut child = task("T-0002", TaskState::Running);
    child.parent_id = Some("T-0001".into());
    let mut parent = task("T-0001", TaskState::Review);
    parent.updated_at = "2026-08-01T09:00:00.000Z".into();
    let mut fixture = Fixture::new(vec![parent, child]);
    fixture.folds = vec![("task:T-0001".into(), true)];
    let model = fixture.model();
    assert_eq!(model.next_label_change_ms(now), Some(60_000));
}

#[test]
fn a_folded_row_still_shows_its_waiting_subtasks() {
    let mut child = task("T-0002", TaskState::Blocked);
    child.parent_id = Some("T-0001".into());
    let mut quiet_child = task("T-0003", TaskState::Ready);
    quiet_child.parent_id = Some("T-0001".into());
    let fixture = Fixture::new(vec![task("T-0001", TaskState::Review), child, quiet_child]);
    let model = fixture.model();
    assert!(!model.is_open("T-0001"));
    assert_eq!(keys(&model), ["repo:a", "task:T-0001", "task:T-0002"]);
}

#[test]
fn says_when_the_filter_hides_every_workspace_of_a_project() {
    let mut fixture = Fixture::new(vec![
        task("T-0001", TaskState::Running),
        task("T-0002", TaskState::Done),
    ]);
    fixture.filter = "nothing like it".into();
    let model = fixture.model();
    assert!(model.filtered);
    assert_eq!(
        model.infos[0],
        GroupInfo {
            count: 0,
            workspaces: 2,
            hidden: 2,
            all_hidden: true,
            gone: false,
            open: true,
            finished_shown: false,
        }
    );
    assert_eq!(
        model.lines,
        [Line::GroupHeader { group: 0 }, Line::NoMatches { group: 0 }]
    );
}

#[test]
fn an_empty_project_offers_to_start_a_task() {
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Done)]);
    fixture.repos.push(repo("b"));
    let model = fixture.model();
    assert!(model.lines.contains(&Line::NoWorkspaces { group: 0 }));
    assert!(model.lines.contains(&Line::Finished { group: 0 }));
    assert!(model.lines.contains(&Line::NoWorkspaces { group: 1 }));
}

#[test]
fn a_group_of_a_project_that_is_gone_is_marked() {
    let mut gone = task("T-0001", TaskState::Running);
    gone.repo = Some("old".into());
    let model = Fixture::new(vec![gone]).model();
    let old = model
        .groups
        .groups
        .iter()
        .position(|group| group.repo.id == "old")
        .unwrap();
    assert!(model.infos[old].gone);
    assert_eq!(model.infos[old].count, 1);
}

#[test]
fn a_filter_that_matches_a_subtask_keeps_its_parent_open() {
    let mut child = task("T-0002", TaskState::Ready);
    child.parent_id = Some("T-0001".into());
    child.title = "Needle".into();
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Review), child]);
    fixture.filter = "needle".into();
    let model = fixture.model();
    assert!(model.is_open("T-0001"));
    assert_eq!(keys(&model), ["repo:a", "task:T-0001"]);
    assert!(model.line_of("task:T-0002").is_some());
}

#[test]
fn a_folded_group_waits_for_its_tasks_to_turn_quiet() {
    let now = parse_iso_ms("2026-10-22T09:05:00.000Z").unwrap_or_default();
    let mut fixture = Fixture::new(vec![task("T-0001", TaskState::Running)]);
    // Shown, the row's age changes at 09:06.
    assert_eq!(fixture.model().next_label_change_ms(now), Some(60_000));
    // Folded, the group's hint changes when the task turns quiet at 09:10.
    fixture.folds = vec![("repo:a".into(), false)];
    assert_eq!(fixture.model().next_label_change_ms(now), Some(300_000));
    fixture.tasks = vec![task("T-0001", TaskState::Done)];
    assert_eq!(fixture.model().next_label_change_ms(now), None);
}
