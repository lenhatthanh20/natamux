//! The rows of the command palette and of the jump palette (`features/palette/palette.ts`, and
//! `capRows`, `nextScope`, `digitOf` of `CommandPalette.tsx`). No window here: the caller passes
//! the commands that can run now, in registry order, and the time.

use std::collections::BTreeSet;

use natamux_proto::tasks::task::Task;

use super::fuzzy::{FuzzyMatch, Range, fuzzy_match};
use crate::sidebar::workspace_groups::DEFAULT_REPO_ID;
use crate::tasks::task_attention::needs_me_tasks;
use crate::tasks::task_sort::task_number;
use crate::terminals::pane_rules::MAX_PANES;

pub const MAX_NEEDS_YOU: usize = 5;
pub const MAX_RECENT: usize = 5;
const MAX_TASKS: usize = 8;
const MAX_COMMANDS: usize = 8;
const MAX_REPOS: usize = 3;
/// More rows than this are never read; a longer list is for typing to narrow, not for scrolling.
pub const MAX_ROWS: usize = 50;

/// A command the palette may list. The caller leaves out commands that cannot run now or that
/// are hidden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteCommand {
    pub id: &'static str,
    /// "Category: Title".
    pub title: String,
    pub key: Option<&'static str>,
    /// Its rank in Suggested; none: not suggested.
    pub suggested: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoItem {
    pub id: String,
    pub name: String,
}

/// "Needs you" narrows the lists to the tasks that wait for the user, a project to its tasks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteScope {
    NeedsYou,
    Repo(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    All,
    Jump,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaletteRow {
    Command {
        command: PaletteCommand,
        ranges: Vec<Range>,
    },
    Task {
        task: Task,
        title_ranges: Vec<Range>,
        id_ranges: Vec<Range>,
    },
    /// An extra terminal of a workspace, in the jump palette.
    Shell {
        task: Task,
        name: String,
    },
    Repo {
        repo: RepoItem,
        ranges: Vec<Range>,
    },
    CreateTask {
        title: String,
    },
    NewWorkspace {
        title: String,
    },
}

impl PaletteRow {
    pub fn key(&self) -> String {
        match self {
            Self::Command { command, .. } => format!("command:{}", command.id),
            Self::Task { task, .. } => format!("task:{}", task.id),
            Self::Shell { task, name } => format!("shell:{}:{name}", task.id),
            Self::Repo { repo, .. } => format!("repo:{}", repo.id),
            Self::CreateTask { .. } => "create-task".to_owned(),
            Self::NewWorkspace { .. } => "new-workspace".to_owned(),
        }
    }

    /// The text of a create row: `Add task “q”` or `Start task “q”`.
    pub fn create_label(&self) -> Option<String> {
        match self {
            Self::CreateTask { title } => Some(format!("Add task “{title}”")),
            Self::NewWorkspace { title } => Some(format!("Start task “{title}”")),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaletteSection {
    pub id: &'static str,
    pub title: Option<&'static str>,
    pub rows: Vec<PaletteRow>,
}

pub struct AllInput<'a> {
    pub query: &'a str,
    pub tasks: &'a [Task],
    pub commands: &'a [PaletteCommand],
    pub repos: &'a [RepoItem],
    /// Ids of the workspaces that were opened, newest first.
    pub recent: &'a [String],
    pub now_ms: i64,
    pub scope: Option<&'a PaletteScope>,
}

fn repo_of(task: &Task) -> &str {
    task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID)
}

pub(super) fn sorted_by_score<T>(mut items: Vec<(T, f64)>) -> Vec<T> {
    items.sort_by(|a, b| b.1.total_cmp(&a.1));
    items.into_iter().map(|(item, _)| item).collect()
}

fn scaled(found: Option<FuzzyMatch>, by: f64) -> Option<FuzzyMatch> {
    found.map(|found| FuzzyMatch {
        score: found.score * by,
        ranges: found.ranges,
    })
}

fn best_of(matches: [Option<FuzzyMatch>; 3]) -> Option<FuzzyMatch> {
    let mut best: Option<FuzzyMatch> = None;
    for found in matches.into_iter().flatten() {
        if best.as_ref().is_none_or(|top| found.score > top.score) {
            best = Some(found);
        }
    }
    best
}

pub(super) fn match_task(task: &Task, query: &str) -> Option<(PaletteRow, f64)> {
    let by_id = fuzzy_match(query, &task.id);
    let by_title = fuzzy_match(query, &task.title);
    let by_branch = task.branch.as_deref().and_then(|b| fuzzy_match(query, b));
    let best = best_of([
        scaled(by_id.clone(), 1.2),
        by_title.clone(),
        scaled(by_branch, 0.8),
    ])?;
    let row = PaletteRow::Task {
        task: task.clone(),
        title_ranges: by_title.map(|found| found.ranges).unwrap_or_default(),
        id_ranges: by_id.map(|found| found.ranges).unwrap_or_default(),
    };
    Some((row, best.score))
}

pub(super) fn scoped(tasks: &[Task], scope: Option<&PaletteScope>, now_ms: i64) -> Vec<Task> {
    match scope {
        None => tasks.to_vec(),
        Some(PaletteScope::Repo(repo)) => tasks
            .iter()
            .filter(|task| repo_of(task) == repo)
            .cloned()
            .collect(),
        Some(PaletteScope::NeedsYou) => {
            let waiting: BTreeSet<String> = needs_me_tasks(tasks, now_ms)
                .into_iter()
                .map(|task| task.id)
                .collect();
            tasks
                .iter()
                .filter(|task| waiting.contains(&task.id))
                .cloned()
                .collect()
        }
    }
}

pub(super) fn task_row(task: &Task) -> PaletteRow {
    PaletteRow::Task {
        task: task.clone(),
        title_ranges: Vec::new(),
        id_ranges: Vec::new(),
    }
}

fn command_row(command: &PaletteCommand, ranges: Vec<Range>) -> PaletteRow {
    PaletteRow::Command {
        command: command.clone(),
        ranges,
    }
}

fn match_commands(commands: &[PaletteCommand], query: &str) -> Vec<(PaletteRow, f64)> {
    commands
        .iter()
        .filter_map(|command| {
            let found = fuzzy_match(query, &command.title)?;
            Some((command_row(command, found.ranges), found.score))
        })
        .collect()
}

fn one_section(
    id: &'static str,
    title: &'static str,
    rows: Vec<PaletteRow>,
) -> Vec<PaletteSection> {
    if rows.is_empty() {
        return Vec::new();
    }
    vec![PaletteSection {
        id,
        title: Some(title),
        rows,
    }]
}

fn commands_only(commands: &[PaletteCommand], query: &str) -> Vec<PaletteSection> {
    // With no query the commands keep the order of the registry.
    let rows = if query.is_empty() {
        commands
            .iter()
            .map(|c| command_row(c, Vec::new()))
            .collect()
    } else {
        sorted_by_score(match_commands(commands, query))
    };
    one_section("commands", "Commands", rows)
}

fn tasks_by_id(input: &AllInput, query: &str) -> Vec<PaletteSection> {
    let needle = query.to_lowercase();
    let mut found: Vec<Task> = scoped(input.tasks, input.scope, input.now_ms)
        .into_iter()
        .filter(|task| task.id.to_lowercase().contains(&needle))
        .collect();
    found.sort_by_key(|task| std::cmp::Reverse(task_number(&task.id)));
    let rows = found.iter().take(MAX_TASKS * 2).map(task_row).collect();
    one_section("tasks", "Tasks", rows)
}

fn empty_query(input: &AllInput) -> Vec<PaletteSection> {
    let waiting: Vec<Task> = needs_me_tasks(
        &scoped(input.tasks, input.scope, input.now_ms),
        input.now_ms,
    )
    .into_iter()
    .take(MAX_NEEDS_YOU)
    .collect();
    let waiting_ids: BTreeSet<&str> = waiting.iter().map(|task| task.id.as_str()).collect();
    let mut sections = Vec::new();
    if !waiting.is_empty() {
        sections.push(PaletteSection {
            id: "needs-you",
            title: Some("Needs you"),
            rows: waiting.iter().map(task_row).collect(),
        });
    }
    if input.scope.is_none() {
        let lately: Vec<PaletteRow> = input
            .recent
            .iter()
            .filter_map(|id| input.tasks.iter().find(|task| &task.id == id))
            .filter(|task| task.workspace.is_some() && !waiting_ids.contains(task.id.as_str()))
            .take(MAX_RECENT)
            .map(task_row)
            .collect();
        if !lately.is_empty() {
            sections.push(PaletteSection {
                id: "recent",
                title: Some("Recent"),
                rows: lately,
            });
        }
        let mut ranked: Vec<&PaletteCommand> = input
            .commands
            .iter()
            .filter(|command| command.suggested.is_some())
            .collect();
        ranked.sort_by_key(|command| command.suggested);
        let suggested: Vec<PaletteRow> = ranked
            .into_iter()
            .map(|command| command_row(command, Vec::new()))
            .collect();
        if !suggested.is_empty() {
            sections.push(PaletteSection {
                id: "suggested",
                title: Some("Suggested"),
                rows: suggested,
            });
        }
    }
    sections
}

fn typed(input: &AllInput, raw: &str) -> Vec<PaletteSection> {
    let task_matches: Vec<(PaletteRow, f64)> = scoped(input.tasks, input.scope, input.now_ms)
        .iter()
        .filter_map(|task| match_task(task, raw))
        .collect();
    let repo_matches: Vec<(PaletteRow, f64)> = input
        .repos
        .iter()
        .filter_map(|repo| {
            let found = fuzzy_match(raw, &repo.name)?;
            let row = PaletteRow::Repo {
                repo: repo.clone(),
                ranges: found.ranges,
            };
            Some((row, found.score))
        })
        .collect();
    // Sections go by their best row; on a tie tasks come first, then commands, then projects.
    let groups = [
        ("tasks", "Tasks", task_matches, MAX_TASKS),
        (
            "commands",
            "Commands",
            match_commands(input.commands, raw),
            MAX_COMMANDS,
        ),
        ("repos", "Projects", repo_matches, MAX_REPOS),
    ];
    let mut sections: Vec<(f64, PaletteSection)> = groups
        .into_iter()
        .filter(|(_, _, matches, _)| !matches.is_empty())
        .map(|(id, title, matches, max)| {
            let top = matches.iter().fold(f64::NEG_INFINITY, |a, m| a.max(m.1));
            let rows = sorted_by_score(matches).into_iter().take(max).collect();
            (
                top,
                PaletteSection {
                    id,
                    title: Some(title),
                    rows,
                },
            )
        })
        .collect();
    sections.sort_by(|a, b| b.0.total_cmp(&a.0));
    if !sections.is_empty() {
        return sections.into_iter().map(|(_, section)| section).collect();
    }
    vec![PaletteSection {
        id: "create",
        title: None,
        rows: vec![
            PaletteRow::CreateTask {
                title: raw.to_owned(),
            },
            PaletteRow::NewWorkspace {
                title: raw.to_owned(),
            },
        ],
    }]
}

/// The rows of the command palette (Mod+K). Empty: what needs the user, recent workspaces and a
/// few commands. `>` searches commands only, `#` searches tasks by id.
pub fn all_sections(input: &AllInput) -> Vec<PaletteSection> {
    let raw = input.query.trim();
    if let Some(rest) = raw.strip_prefix('>') {
        return commands_only(input.commands, rest.trim());
    }
    if let Some(rest) = raw.strip_prefix('#') {
        return tasks_by_id(input, rest.trim());
    }
    if raw.is_empty() {
        return empty_query(input);
    }
    typed(input, raw)
}

pub fn flat_rows(sections: &[PaletteSection]) -> Vec<PaletteRow> {
    sections
        .iter()
        .flat_map(|section| section.rows.iter().cloned())
        .collect()
}

/// The digit that picks a row with Alt+1 to Alt+9, or None past the ninth.
pub fn digit_of(index: usize) -> Option<usize> {
    (index < 9).then_some(index + 1)
}

/// The first `MAX_ROWS` rows; the sections after the cap are dropped.
pub fn cap_rows(sections: Vec<PaletteSection>) -> Vec<PaletteSection> {
    let mut left = MAX_ROWS;
    let mut capped = Vec::new();
    for mut section in sections {
        if left == 0 {
            break;
        }
        section.rows.truncate(left);
        left -= section.rows.len();
        capped.push(section);
    }
    capped
}

/// The projects that have a task, in project order: the ones Tab cycles over.
pub fn repos_with_tasks(repos: &[RepoItem], tasks: &[Task]) -> Vec<RepoItem> {
    repos
        .iter()
        .filter(|repo| tasks.iter().any(|task| repo_of(task) == repo.id))
        .cloned()
        .collect()
}

/// Tab goes from no scope to Needs you, then to each project that has a task, then back.
pub fn next_scope(
    scope: Option<&PaletteScope>,
    repos_with_tasks: &[RepoItem],
) -> Option<PaletteScope> {
    let mut order: Vec<Option<PaletteScope>> = vec![None, Some(PaletteScope::NeedsYou)];
    order.extend(
        repos_with_tasks
            .iter()
            .map(|repo| Some(PaletteScope::Repo(repo.id.clone()))),
    );
    let next = order
        .iter()
        .position(|item| item.as_ref() == scope)
        .map_or(0, |at| at + 1)
        % order.len();
    order.swap_remove(next)
}

/// The name on the scope chip.
pub fn scope_name(scope: &PaletteScope, repos: &[RepoItem]) -> String {
    match scope {
        PaletteScope::NeedsYou => "Needs you".to_owned(),
        PaletteScope::Repo(id) => repos
            .iter()
            .find(|repo| repo.id == *id)
            .map_or_else(|| id.clone(), |repo| repo.name.clone()),
    }
}

/// What the list says when it has no row. `repo_name` is the name of the repo scope.
pub fn empty_text(
    mode: PaletteMode,
    scope: Option<&PaletteScope>,
    query: &str,
    repo_name: &str,
) -> String {
    if query.trim().starts_with('>') {
        return "No command matches.".to_owned();
    }
    match scope {
        Some(PaletteScope::NeedsYou) => "Nothing needs you.".to_owned(),
        Some(PaletteScope::Repo(_)) => format!("No workspace in {repo_name}."),
        None if mode == PaletteMode::Jump => "No workspace yet.".to_owned(),
        None => "Nothing to show yet. Type to search.".to_owned(),
    }
}

/// The Agents grid after a task joins it: unchanged when it is there already, an error when the
/// grid is full.
pub fn add_to_agents(chosen: &[String], id: &str) -> Result<Vec<String>, String> {
    let mut next = chosen.to_vec();
    if next.iter().any(|each| each == id) {
        return Ok(next);
    }
    if next.len() >= MAX_PANES {
        return Err(format!(
            "The Agents grid holds {MAX_PANES} panes. Close one to add {id}."
        ));
    }
    next.push(id.to_owned());
    Ok(next)
}

#[cfg(test)]
#[path = "palette_rows_tests.rs"]
mod tests;
// The jump rules are tested here too: they share the helpers of `tests`.
#[cfg(test)]
#[path = "jump_rows_tests.rs"]
mod jump_tests;
