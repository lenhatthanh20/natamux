use natamux_proto::terminals::list::TerminalLane;

use super::*;

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

#[test]
fn only_the_open_workspace_shows_its_terminal_rows() {
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
    assert!(model.can_fold("T-0002"));
    let shown: Vec<String> = keys(&model);
    assert!(shown.contains(&"agent:T-0001:shell:1".to_owned()));
    assert!(shown.contains(&"agent:T-0001:shell:2".to_owned()));
    assert!(!shown.iter().any(|key| key.starts_with("agent:T-0002")));
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
    assert_eq!(keys(&model), ["repo:a", "task:T-0001", "task:T-0002"]);
}
