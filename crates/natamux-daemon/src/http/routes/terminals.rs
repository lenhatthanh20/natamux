//! The terminal routes of part 1 (`routes/terminals.ts`), but the socket (`terminal_socket.rs`).

use hyper::StatusCode;
use natamux_proto::terminals::query::is_terminal_name;
use serde_json::{Map, Value, json};

use crate::http::errors::{Answer, Issue, invalid, json_answer, terminal_error};
use crate::http::server::AppState;
use crate::terminals::shell_files::read_ended_shells;

const TASK_ID_MESSAGE: &str = "Task id looks like T-42 or ABC-42";
const TERMINAL_MESSAGE: &str = "A terminal is named by a number";

/// The rules of `taskIdSchema`, as zod reports them.
pub fn task_id_issues(id: &str) -> Vec<Issue> {
    let mut issues = Vec::new();
    if id.encode_utf16().count() > 32 {
        issues.push(Issue {
            path: "id",
            message: "Too big: expected string to have <=32 characters",
            detail: json!({ "origin": "string", "code": "too_big", "maximum": 32, "inclusive": true }),
        });
    }
    if !looks_like_task_id(id) {
        issues.push(Issue {
            path: "id",
            message: TASK_ID_MESSAGE,
            detail: json!({
                "origin": "string", "code": "invalid_format", "format": "regex",
                "pattern": "/^[A-Z]+-\\d+$/",
            }),
        });
    }
    issues
}

/// `^[A-Z]+-\d+$` without the length limit.
fn looks_like_task_id(id: &str) -> bool {
    id.split_once('-').is_some_and(|(letters, digits)| {
        !letters.is_empty()
            && letters.bytes().all(|byte| byte.is_ascii_uppercase())
            && !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub fn terminal_issues(terminal: &str) -> Vec<Issue> {
    if is_terminal_name(terminal) {
        return Vec::new();
    }
    vec![Issue {
        path: "terminal",
        message: TERMINAL_MESSAGE,
        detail: json!({
            "origin": "string", "code": "invalid_format", "format": "regex",
            "pattern": "/^[1-9]\\d{0,2}$/",
        }),
    }]
}

fn ok(body: Value) -> Answer {
    json_answer(StatusCode::OK, &body)
}

/// The last screens of the shells of a done or cancelled task, for its wrap-up.
pub fn ended_shells(state: &AppState, id: &str) -> Answer {
    let issues = task_id_issues(id);
    if !issues.is_empty() {
        return invalid(&issues);
    }
    ok(json!({ "shells": read_ended_shells(&state.data_dir, id) }))
}

/// The open shells, and those that wait to come back after a restart, so their panes show.
/// No stage runs an agent in part 1: `agents` is empty and `lanes` has none.
pub fn list_terminals(state: &AppState, id: &str) -> Answer {
    let issues = task_id_issues(id);
    if !issues.is_empty() {
        return invalid(&issues);
    }
    let open = state.terminals.list(id);
    let mut terminals = open.clone();
    for name in state.open_shells.waiting(id) {
        if !terminals.contains(&name) {
            terminals.push(name);
        }
    }
    terminals.sort_by_key(|name| name.parse::<u32>().unwrap_or(u32::MAX));
    let mut programs = Map::new();
    for name in &open {
        let program = state.terminals.foreground(id, name).map(|program| {
            let base = program.rsplit(['/', '\\']).next().unwrap_or(&program);
            base.to_string()
        });
        programs.insert(name.clone(), json!(program));
    }
    let mut restoring = Map::new();
    for (name, agent) in state.open_shells.restoring(id) {
        restoring.insert(name, json!(agent));
    }
    ok(json!({
        "terminals": terminals,
        "agents": [],
        "lanes": {},
        "programs": programs,
        "restoring": restoring,
    }))
}

pub fn close_terminal(state: &AppState, id: &str, terminal: &str) -> Answer {
    let mut issues = task_id_issues(id);
    issues.extend(terminal_issues(terminal));
    if !issues.is_empty() {
        return invalid(&issues);
    }
    state.terminals.close_one(id, terminal);
    ok(json!({ "terminals": state.terminals.list(id) }))
}

pub fn ticket(state: &AppState, id: &str) -> Answer {
    let issues = task_id_issues(id);
    if !issues.is_empty() {
        return invalid(&issues);
    }
    match state.terminals.issue_ticket(id) {
        Ok(ticket) => ok(json!({ "ticket": ticket })),
        Err(error) => terminal_error(&error),
    }
}
