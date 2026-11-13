//! The rows of the jump palette (`jumpSections` of `features/palette/palette.ts`): workspaces and
//! their terminals.

use std::collections::BTreeMap;

use natamux_proto::tasks::task::{Task, TaskState};

use super::palette_rows::{
    MAX_ROWS, PaletteMode, PaletteRow, PaletteScope, PaletteSection, scoped, task_row,
};
use super::task_match::ranked_tasks;
use crate::tasks::task_attention::is_quiet;
use crate::tasks::task_sort::{state_rank, task_number};

pub struct JumpInput<'a> {
    pub query: &'a str,
    pub tasks: &'a [Task],
    pub recent: &'a [String],
    pub now_ms: i64,
    pub scope: Option<&'a PaletteScope>,
    /// Task id to the names of its open terminals, in order.
    pub terminals: &'a BTreeMap<String, Vec<String>>,
}

/// The jump palette reads the terminals of the workspaces itself up to this many, as the web.
const MAX_TERMINAL_READS: usize = 20;

/// The tasks whose terminals the palette asks for while it is open (the `enabled` of the web's
/// shell queries): in the jump palette, every task with a workspace, unless there are more
/// than 20; the command palette lists no terminals.
pub fn terminal_reads(mode: PaletteMode, tasks: &[Task]) -> Vec<String> {
    let workspaces: Vec<String> = tasks
        .iter()
        .filter(|task| task.workspace.is_some())
        .map(|task| task.id.clone())
        .collect();
    if mode == PaletteMode::Jump && workspaces.len() <= MAX_TERMINAL_READS {
        workspaces
    } else {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpGroup {
    NeedsYou,
    Running,
    Other,
}

const WAITING_RANK: [TaskState; 4] = [
    TaskState::NeedsInput,
    TaskState::Review,
    TaskState::Failed,
    TaskState::Blocked,
];

/// Tasks that need the user (what asks comes first), then running ones, then the rest.
pub fn jump_group(task: &Task, now_ms: i64) -> JumpGroup {
    if task.state == TaskState::Running {
        return if is_quiet(task, now_ms) {
            JumpGroup::NeedsYou
        } else {
            JumpGroup::Running
        };
    }
    if WAITING_RANK.contains(&task.state) {
        JumpGroup::NeedsYou
    } else {
        JumpGroup::Other
    }
}

fn waiting_rank(task: &Task) -> usize {
    WAITING_RANK
        .iter()
        .position(|state| *state == task.state)
        .unwrap_or(WAITING_RANK.len())
}

fn with_shells(task: &Task, row: PaletteRow, input: &JumpInput) -> Vec<PaletteRow> {
    let shells = input
        .terminals
        .get(&task.id)
        .into_iter()
        .flatten()
        .filter(|name| name.as_str() != "1")
        .map(|name| PaletteRow::Shell {
            task: task.clone(),
            name: name.clone(),
        });
    std::iter::once(row).chain(shells).collect()
}

fn jump_typed(workspaces: &[&Task], query: &str, input: &JumpInput) -> Vec<PaletteSection> {
    // Only the first MAX_ROWS rows are ever shown, so no task past them is cloned into a row.
    let mut rows: Vec<PaletteRow> = Vec::new();
    for (task, found) in ranked_tasks(workspaces, query) {
        if rows.len() >= MAX_ROWS {
            break;
        }
        rows.extend(with_shells(task, found.into_row(task), input));
    }
    rows.truncate(MAX_ROWS);
    if rows.is_empty() {
        return vec![PaletteSection {
            id: "create",
            title: None,
            rows: vec![PaletteRow::NewWorkspace {
                title: query.to_owned(),
            }],
        }];
    }
    vec![PaletteSection {
        id: "workspaces",
        title: Some("Workspaces"),
        rows,
    }]
}

/// The rows of the jump palette (Mod+J): workspaces and their terminals.
pub fn jump_sections(input: &JumpInput) -> Vec<PaletteSection> {
    let now_ms = input.now_ms;
    // The needs-you pill counts the tasks without a workspace too, so its list shows them.
    let all_tasks = input.scope == Some(&PaletteScope::NeedsYou);
    let workspaces = scoped(
        input
            .tasks
            .iter()
            .filter(|task| all_tasks || task.workspace.is_some()),
        input.scope,
        now_ms,
    );
    let query = input.query.trim();
    if !query.is_empty() {
        return jump_typed(&workspaces, query, input);
    }
    jump_idle(&workspaces, input)
}

/// The sections with nothing typed: needs you, running, recent, the rest.
fn jump_idle(workspaces: &[&Task], input: &JumpInput) -> Vec<PaletteSection> {
    let now_ms = input.now_ms;
    let recent_rank = |task: &Task| {
        input
            .recent
            .iter()
            .position(|id| *id == task.id)
            .unwrap_or(usize::MAX)
    };
    let by_recent = |a: &Task, b: &Task| {
        recent_rank(a)
            .cmp(&recent_rank(b))
            .then_with(|| state_rank(a.state).cmp(&state_rank(b.state)))
            .then_with(|| task_number(&b.id).cmp(&task_number(&a.id)))
    };
    let group = |wanted: JumpGroup| -> Vec<&Task> {
        workspaces
            .iter()
            .copied()
            .filter(|task| jump_group(task, now_ms) == wanted)
            .collect()
    };
    let mut waiting = group(JumpGroup::NeedsYou);
    waiting.sort_by(|a, b| {
        waiting_rank(a)
            .cmp(&waiting_rank(b))
            .then_with(|| by_recent(a, b))
    });
    let mut running = group(JumpGroup::Running);
    running.sort_by(|a, b| by_recent(a, b));
    let mut rest = group(JumpGroup::Other);
    rest.sort_by(|a, b| by_recent(a, b));
    let (recent_rest, older): (Vec<&Task>, Vec<&Task>) = rest
        .into_iter()
        .partition(|task| input.recent.contains(&task.id));

    // Only the first MAX_ROWS rows are ever shown, so later tasks are not cloned into rows.
    let mut left = MAX_ROWS;
    let mut sections = Vec::new();
    for (id, title, tasks) in [
        ("needs-you", "Needs you", waiting),
        ("running", "Running", running),
        ("recent", "Recent", recent_rest),
        ("workspaces", "Workspaces", older),
    ] {
        if left == 0 {
            break;
        }
        if tasks.is_empty() {
            continue;
        }
        let mut rows: Vec<PaletteRow> = Vec::new();
        for task in tasks {
            if rows.len() >= left {
                break;
            }
            rows.extend(with_shells(task, task_row(task), input));
        }
        rows.truncate(left);
        left -= rows.len();
        sections.push(PaletteSection {
            id,
            title: Some(title),
            rows,
        });
    }
    sections
}
