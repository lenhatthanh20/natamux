//! The rows under a task in the sidebar (`shellRows` and friends in `sidebar-tree.ts`, and
//! `terminalState` in `lib/panes.ts`).

use std::cmp::Ordering;
use std::collections::BTreeSet;

use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::{TerminalLane, TerminalList};

use crate::sidebar::sidebar_tree::{agent_label, lane_state, stage_name, state_label};

/// A row under a task: one of its shells, or a parallel stage that has no shell yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellRow {
    pub key: String,
    /// The stage the agent works on, "Terminal 2" for an agent on no stage, or the program of a
    /// shell with no agent.
    pub label: String,
    pub agent: Option<String>,
    /// None: a shell with no agent, or an agent whose state is not known, such as one started by
    /// hand before the daemon restarted.
    pub state: Option<TaskState>,
    /// The terminal to focus. None: the task's first shell.
    pub shell: Option<String>,
    /// A shell with no agent in it.
    pub plain: bool,
}

/// The state of an agent terminal, from the task, which live updates keep fresh. An agent on no
/// stage has the state its hooks gave, or none.
pub fn terminal_state(task: &Task, lane: &TerminalLane) -> Option<TaskState> {
    let Some(stage) = lane.stage.as_deref() else {
        return lane.state;
    };
    lane_state(task, stage).or(if stage == task.stage {
        Some(task.state)
    } else {
        lane.state
    })
}

// Shells are numbers; a name that is not one sorts after by its text.
fn shell_order(a: &str, b: &str) -> Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) if x != y => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    }
}

/// The rows under a task: each open shell in shell order, then the parallel stages no shell works
/// on yet. Before the shells are read (`shells` is None), only the stages.
pub fn shell_rows(task: &Task, shells: Option<&TerminalList>) -> Vec<ShellRow> {
    let mut names: Vec<&String> = shells
        .map(|list| list.terminals.iter().collect())
        .unwrap_or_default();
    names.sort_by(|a, b| shell_order(a, b));
    let mut rows: Vec<ShellRow> = names
        .into_iter()
        .map(|name| {
            let lane = shells.and_then(|list| list.lanes.get(name));
            // An agent that comes back after a restart once the task is opened.
            let restoring = shells
                .and_then(|list| list.restoring.get(name))
                .and_then(Option::as_ref);
            let key = format!("shell:{name}");
            let shell = Some(name.clone());
            match (lane, restoring) {
                (None, Some(agent)) => ShellRow {
                    key,
                    label: format!("Terminal {name}"),
                    agent: Some(agent.clone()),
                    state: None,
                    shell,
                    plain: false,
                },
                (None, None) => {
                    let program = shells
                        .and_then(|list| list.programs.get(name))
                        .and_then(Option::as_ref);
                    ShellRow {
                        key,
                        label: program.cloned().unwrap_or_else(|| "Terminal".into()),
                        agent: None,
                        state: None,
                        shell,
                        plain: true,
                    }
                }
                (Some(lane), _) => ShellRow {
                    key,
                    label: lane
                        .stage
                        .as_deref()
                        .map_or_else(|| format!("Terminal {name}"), stage_name),
                    agent: lane.agent.clone().flatten(),
                    state: terminal_state(task, lane),
                    shell,
                    plain: false,
                },
            }
        })
        .collect();
    let covered: BTreeSet<&str> = shells
        .map(|list| {
            list.lanes
                .values()
                .filter_map(|lane| lane.stage.as_deref())
                .collect()
        })
        .unwrap_or_default();
    for stage in task
        .lanes
        .keys()
        .filter(|stage| !covered.contains(stage.as_str()))
    {
        rows.push(ShellRow {
            key: format!("lane:{stage}"),
            label: stage_name(stage),
            agent: None,
            state: lane_state(task, stage),
            shell: None,
            plain: false,
        });
    }
    rows
}

/// "Implement, Claude Code, Terminal 1, Running"; "Terminal 2, zsh" for a terminal with no agent.
pub fn shell_row_name(row: &ShellRow) -> String {
    if row.plain {
        return format!(
            "Terminal {}, {}",
            row.shell.as_deref().unwrap_or_default(),
            row.label
        );
    }
    let shell = row
        .shell
        .as_deref()
        .filter(|shell| row.label != format!("Terminal {shell}"))
        .map(|shell| format!("Terminal {shell}"));
    let agent = row.agent.as_deref().map(|id| agent_label(Some(id)));
    let state = row.state.map_or("State not known", state_label).to_owned();
    [Some(row.label.clone()), agent, shell, Some(state)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

/// "T1" at the right of a row, where its name does not already say the terminal.
pub fn shell_tag(row: &ShellRow) -> Option<String> {
    let shell = row.shell.as_deref()?;
    (row.label != format!("Terminal {shell}")).then(|| format!("T{shell}"))
}

/// The task glyph spins while any of its agents works, also when the task waits on another one.
pub fn task_glyph_state(task: &Task, rows: &[ShellRow]) -> TaskState {
    let working = rows.iter().any(|row| row.state == Some(TaskState::Running))
        || task
            .lanes
            .keys()
            .any(|stage| lane_state(task, stage) == Some(TaskState::Running));
    if working {
        TaskState::Running
    } else {
        task.state
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn make_task(overrides: Value) -> Task {
        let mut base = json!({
            "id": "T-0001", "title": "Example", "state": "running", "workspace": "/work/T-0001",
            "createdAt": "2026-10-08T09:00:00.000Z", "updatedAt": "2026-10-08T09:00:00.000Z"
        });
        if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), overrides) {
            base.extend(extra);
        }
        serde_json::from_value(base).unwrap_or_default()
    }

    // The answer of GET /tasks/:id/terminals; `terminals` defaults to the names of lanes and
    // programs.
    fn shells(lanes: Value, programs: Value, terminals: Option<Value>) -> TerminalList {
        let names = |value: &Value| -> Vec<String> {
            value
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default()
        };
        let mut all = names(&lanes);
        let extra: Vec<String> = names(&programs)
            .into_iter()
            .filter(|name| !all.contains(name))
            .collect();
        all.extend(extra);
        serde_json::from_value(json!({
            "terminals": terminals.unwrap_or_else(|| json!(all)),
            "lanes": lanes, "programs": programs
        }))
        .unwrap_or_default()
    }

    fn parallel_task() -> Task {
        make_task(json!({
            "stage": "checks", "state": "needs-input",
            "lanes": {"alpha": "running", "beta": "needs-input"}
        }))
    }

    fn row(
        label: &str,
        shell: Option<&str>,
        agent: Option<&str>,
        state: Option<TaskState>,
    ) -> ShellRow {
        ShellRow {
            key: "k".into(),
            label: label.into(),
            agent: agent.map(Into::into),
            state,
            shell: shell.map(Into::into),
            plain: false,
        }
    }

    #[test]
    fn stands_in_a_row_per_parallel_stage_before_the_shells_are_known() {
        let rows = shell_rows(&parallel_task(), None);
        let expect = |key: &str, label: &str, state| ShellRow {
            key: key.into(),
            label: label.into(),
            agent: None,
            state: Some(state),
            shell: None,
            plain: false,
        };
        assert_eq!(
            rows,
            [
                expect("lane:alpha", "Alpha", TaskState::Running),
                expect("lane:beta", "Beta", TaskState::NeedsInput)
            ]
        );
    }

    #[test]
    fn gives_each_shell_a_row_in_shell_order_with_its_stage_agent_and_state() {
        let list = shells(
            json!({
                "1": {"stage": "alpha", "state": null, "agent": "claude-code"},
                "2": {"stage": "beta", "state": null, "agent": "codex"}
            }),
            json!({"10": "zsh"}),
            Some(json!(["10", "2", "1"])),
        );
        let rows = shell_rows(&parallel_task(), Some(&list));
        type Seen<'a> = (&'a str, &'a str, Option<&'a str>, Option<TaskState>, bool);
        let seen: Vec<Seen> = rows
            .iter()
            .map(|r| {
                (
                    r.key.as_str(),
                    r.label.as_str(),
                    r.agent.as_deref(),
                    r.state,
                    r.plain,
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (
                    "shell:1",
                    "Alpha",
                    Some("claude-code"),
                    Some(TaskState::Running),
                    false
                ),
                (
                    "shell:2",
                    "Beta",
                    Some("codex"),
                    Some(TaskState::NeedsInput),
                    false
                ),
                ("shell:10", "zsh", None, None, true),
            ]
        );
    }

    #[test]
    fn names_an_agent_on_no_stage_after_its_shell_with_the_state_its_hooks_gave() {
        let list = shells(
            json!({
                "1": {"stage": "spec", "state": "running", "agent": "claude-code"},
                "2": {"stage": null, "state": null, "agent": "cursor"},
                "3": {"stage": null, "state": "needs-input", "agent": "cursor"}
            }),
            json!({}),
            None,
        );
        let rows = shell_rows(&make_task(json!({"stage": "spec"})), Some(&list));
        let seen: Vec<(&str, Option<&str>, Option<TaskState>)> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.agent.as_deref(), r.state))
            .collect();
        assert_eq!(
            seen,
            [
                ("Spec", Some("claude-code"), Some(TaskState::Running)),
                ("Terminal 2", Some("cursor"), None),
                ("Terminal 3", Some("cursor"), Some(TaskState::NeedsInput)),
            ]
        );
    }

    #[test]
    fn names_a_plain_terminal_after_its_program_else_terminal() {
        let list = shells(json!({}), json!({"1": "bash", "2": null}), None);
        let rows = shell_rows(&make_task(json!({})), Some(&list));
        let seen: Vec<(&str, bool, Option<&str>)> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.plain, r.shell.as_deref()))
            .collect();
        assert_eq!(
            seen,
            [("bash", true, Some("1")), ("Terminal", true, Some("2"))]
        );
    }

    #[test]
    fn shows_a_shell_whose_agent_comes_back_after_a_restart_as_an_agent_row() {
        let mut list = shells(json!({}), json!({}), Some(json!(["2", "3"])));
        list.restoring.insert("2".into(), Some("codex".into()));
        list.restoring.insert("3".into(), None);
        let rows = shell_rows(&make_task(json!({})), Some(&list));
        let seen: Vec<(&str, Option<&str>, bool, Option<TaskState>)> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.agent.as_deref(), r.plain, r.state))
            .collect();
        assert_eq!(
            seen,
            [
                ("Terminal 2", Some("codex"), false, None),
                ("Terminal", None, true, None)
            ]
        );
    }

    #[test]
    fn keeps_a_parallel_stage_that_no_shell_works_on() {
        let list = shells(
            json!({"1": {"stage": "alpha", "state": null}}),
            json!({}),
            None,
        );
        let rows = shell_rows(&parallel_task(), Some(&list));
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["shell:1", "lane:beta"]);
    }

    #[test]
    fn says_the_stage_the_agent_the_shell_and_the_state() {
        let named = row(
            "Implement",
            Some("1"),
            Some("claude-code"),
            Some(TaskState::Running),
        );
        assert_eq!(
            shell_row_name(&named),
            "Implement, Claude Code, Terminal 1, Running"
        );
        let by_hand = row("Terminal 2", Some("2"), Some("cursor"), None);
        assert_eq!(
            shell_row_name(&by_hand),
            "Terminal 2, Cursor, State not known"
        );
        // Test spec C16: a known state is said as before, with no "State not known".
        let waiting = ShellRow {
            state: Some(TaskState::NeedsInput),
            ..by_hand.clone()
        };
        assert_eq!(shell_row_name(&waiting), "Terminal 2, Cursor, Needs input");
        let failed = ShellRow {
            state: Some(TaskState::Failed),
            ..by_hand
        };
        assert_eq!(shell_row_name(&failed), "Terminal 2, Cursor, Failed");
        let plain = ShellRow {
            plain: true,
            ..row("zsh", Some("2"), None, None)
        };
        assert_eq!(shell_row_name(&plain), "Terminal 2, zsh");
        let lane = row("Beta", None, None, Some(TaskState::Running));
        assert_eq!(shell_row_name(&lane), "Beta, Running");
    }

    #[test]
    fn tags_a_shell_only_where_the_name_does_not_say_it() {
        let state = Some(TaskState::Running);
        assert_eq!(
            shell_tag(&row("Implement", Some("1"), Some("claude-code"), state)),
            Some("T1".into())
        );
        assert_eq!(
            shell_tag(&row("Terminal 2", Some("2"), Some("claude-code"), state)),
            None
        );
        let plain = ShellRow {
            plain: true,
            ..row("zsh", Some("2"), None, None)
        };
        assert_eq!(shell_tag(&plain), Some("T2".into()));
        assert_eq!(
            shell_tag(&row("Implement", None, Some("claude-code"), state)),
            None
        );
    }

    fn rows_in(states: &[Option<TaskState>]) -> Vec<ShellRow> {
        states
            .iter()
            .map(|state| row("x", Some("1"), Some("claude-code"), *state))
            .collect()
    }

    #[test]
    fn spins_while_any_row_runs_and_else_shows_the_task_state() {
        use TaskState::*;
        let waiting = make_task(json!({"state": "needs-input"}));
        assert_eq!(
            task_glyph_state(&make_task(json!({})), &rows_in(&[Some(Running)])),
            Running
        );
        assert_eq!(
            task_glyph_state(&waiting, &rows_in(&[Some(NeedsInput), Some(Running)])),
            Running
        );
        assert_eq!(
            task_glyph_state(&waiting, &rows_in(&[Some(NeedsInput), Some(Done)])),
            NeedsInput
        );
        assert_eq!(
            task_glyph_state(&make_task(json!({"state": "review"})), &[]),
            Review
        );
    }

    #[test]
    fn spins_for_a_running_parallel_stage_while_the_shells_are_not_read() {
        let task = make_task(
            json!({"state": "needs-input", "lanes": {"a": "needs-input", "b": "running"}}),
        );
        assert_eq!(task_glyph_state(&task, &[]), TaskState::Running);
    }

    #[test]
    fn takes_the_state_of_a_terminal_from_its_stage_in_the_task() {
        let task = make_task(json!({"stage": "spec", "state": "review", "lanes": {"a": "failed"}}));
        let lane = |stage: Option<&str>, state| TerminalLane {
            stage: stage.map(Into::into),
            state,
            agent: None,
        };
        assert_eq!(
            terminal_state(&task, &lane(Some("a"), None)),
            Some(TaskState::Failed)
        );
        assert_eq!(
            terminal_state(&task, &lane(Some("spec"), None)),
            Some(TaskState::Review)
        );
        assert_eq!(
            terminal_state(&task, &lane(Some("other"), Some(TaskState::Done))),
            Some(TaskState::Done)
        );
        assert_eq!(
            terminal_state(&task, &lane(None, Some(TaskState::Running))),
            Some(TaskState::Running)
        );
    }
}
