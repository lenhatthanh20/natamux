//! The workspace routes (`routes/workspaces.ts`): make and remove a workspace, the quick start of
//! a task, fan out, commit, rollback, push and pull requests. The routes that read a workspace are
//! in `workspace_files.rs`. Git and gh calls block, so each one runs on a blocking thread.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::tasks::inputs::{Upload, attachments_body, task_id_param};
use natamux_core::tasks::task::{task_context, title};
use natamux_core::zod::{
    Issue, Issues, Length, Rule, ZodError, array, invalid_type, object, one_of, string,
};
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;
use natamux_proto::tasks::task::TaskContext;
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::http::errors::{
    Answer, internal_error, json_answer, watch_error, workspace_error, zod_answer,
};
use crate::http::server::{AppState, query_value};
use crate::stages::act::StageActionInput;
use crate::tasks::upload::decode_attachments;
use crate::workspaces::branch_name::is_branch_name;
use crate::workspaces::errors::WorkspaceResult;
use crate::workspaces::fan_out::{FanOutParts, MAX_TRIES, fan_out};
use crate::workspaces::new_workspace::{NewWorkspaceInput, NewWorkspaceParts, new_workspace};

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Runs workspace work on a blocking thread and answers with its JSON, or its error.
pub async fn answer(
    status: StatusCode,
    work: impl FnOnce() -> WorkspaceResult<Value> + Send + 'static,
) -> Answer {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(body)) => json_answer(status, &body),
        Ok(Err(error)) => workspace_error(&error),
        Err(_) => internal_error(),
    }
}

fn param(id: &str) -> Result<String, Box<Answer>> {
    task_id_param(id).map_err(|error| Box::new(zod_answer(&error)))
}

/// The lowest value a number may have.
#[derive(Clone, Copy)]
pub enum Least {
    /// `.min(n)`.
    AtLeast(u64),
    /// `.positive()` is `Above(0)`.
    Above(u64),
}

/// `z.number().int()` with its bounds, as zod 4 words them.
pub fn integer(
    value: Option<&Value>,
    path: &[Value],
    least: Least,
    most: Option<u64>,
    issues: &mut Issues,
) -> Option<u64> {
    let Some(Value::Number(number)) = value else {
        issues.push(invalid_type("number", value, path));
        return None;
    };
    let float = number.as_f64().unwrap_or(f64::NAN);
    if !float.is_finite() || float.fract() != 0.0 {
        issues.push(invalid_type("int", value, path));
        return None;
    }
    let before = issues.len();
    let (minimum, inclusive) = match least {
        Least::AtLeast(minimum) => (minimum, true),
        Least::Above(minimum) => (minimum, false),
    };
    let below = if inclusive {
        float < minimum as f64
    } else {
        float <= minimum as f64
    };
    if below {
        let sign = if inclusive { ">=" } else { ">" };
        issues.push(Issue::new(
            vec![
                ("origin", json!("number")),
                ("code", json!("too_small")),
                ("minimum", json!(minimum)),
                ("inclusive", json!(inclusive)),
            ],
            path,
            format!("Too small: expected number to be {sign}{minimum}"),
        ));
    }
    if let Some(maximum) = most.filter(|maximum| float > *maximum as f64) {
        issues.push(Issue::new(
            vec![
                ("origin", json!("number")),
                ("code", json!("too_big")),
                ("maximum", json!(maximum)),
                ("inclusive", json!(true)),
            ],
            path,
            format!("Too big: expected number to be <={maximum}"),
        ));
    }
    (issues.len() == before).then_some(float as u64)
}

pub async fn diff_stats(state: Arc<AppState>) -> Answer {
    answer(StatusCode::OK, move || {
        let stats: serde_json::Map<String, Value> = state
            .workspaces
            .diff_stats()?
            .into_iter()
            .map(|(id, stat)| {
                let each =
                    json!({ "files": stat.files, "added": stat.added, "removed": stat.removed });
                (id, each)
            })
            .collect();
        Ok(json!({ "stats": stats }))
    })
    .await
}

pub fn preparing(state: &AppState) -> Answer {
    let steps: serde_json::Map<String, Value> = state
        .workspaces
        .preparing()
        .into_iter()
        .map(|(id, step)| (id, json!(step)))
        .collect();
    json_answer(StatusCode::OK, &json!({ "preparing": steps }))
}

/// `base: z.string().min(1).max(255).optional()`.
fn base_field(map: &serde_json::Map<String, Value>, issues: &mut Issues) -> Option<String> {
    map.get("base").and_then(|base| {
        string(
            Some(base),
            &[json!("base")],
            &[Rule::Min(1), Rule::Max(255)],
            issues,
        )
    })
}

/// `branch: z.string().min(1).max(255).refine(isBranchName).optional()`: a name git takes.
pub fn branch_field(map: &serde_json::Map<String, Value>, issues: &mut Issues) -> Option<String> {
    let path = [json!("branch")];
    let rules = [Rule::Min(1), Rule::Max(255)];
    let branch = string(Some(map.get("branch")?), &path, &rules, issues)?;
    if !is_branch_name(&branch) {
        issues.push(Issue::custom(&path, "Invalid branch name"));
        return None;
    }
    Some(branch)
}

/// `createWorkspaceBodySchema`: an optional body with an optional base and branch.
fn create_body(body: Option<&Value>) -> Result<(Option<String>, Option<String>), ZodError> {
    let Some(body) = body else {
        return Ok((None, None));
    };
    let mut issues = Issues::default();
    let map = object(Some(body), &[], &mut issues);
    let base = map.and_then(|map| base_field(map, &mut issues));
    let branch = map.and_then(|map| branch_field(map, &mut issues));
    issues.into_result(Some((base, branch)))
}

pub async fn create(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let (base, branch) = match create_body(body.as_ref()) {
        Ok(fields) => fields,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::CREATED, move || {
        let task = state
            .workspaces
            .create(&id, base.as_deref(), branch.as_deref())?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

/// `removeWorkspaceQuerySchema`: `deleteBranch` and `force` are `true` or `false`, else false.
pub fn bool_query(query: &str, name: &str) -> Result<bool, ZodError> {
    let mut issues = Issues::default();
    let wanted = match query_value(query, name) {
        None => Some(false),
        Some(value) => one_of(
            Some(&value),
            &[json!(name)],
            &["true", "false"],
            &mut issues,
        )
        .map(|text| text == "true"),
    };
    issues.into_result(wanted)
}

pub async fn remove(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let (delete_branch, force) = match bool_query(query, "deleteBranch")
        .and_then(|delete_branch| Ok((delete_branch, bool_query(query, "force")?)))
    {
        Ok(wanted) => wanted,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        let removed = if force {
            state.workspaces.remove_anyway(&id, delete_branch)?
        } else {
            state.workspaces.remove(&id, delete_branch)?
        };
        Ok(json!({ "task": to_value(&removed.task), "branchDeleted": removed.branch_deleted }))
    })
    .await
}

/// `fanOutBodySchema`: how many copies, and whether each starts at once.
fn fan_out_body(body: Option<&Value>) -> Result<(usize, bool), ZodError> {
    let mut issues = Issues::default();
    let parsed = object(body, &[], &mut issues).and_then(|map| {
        let count = integer(
            map.get("count"),
            &[json!("count")],
            Least::AtLeast(1),
            Some(MAX_TRIES as u64),
            &mut issues,
        );
        let start = match map.get("start") {
            None => Some(false),
            Some(Value::Bool(start)) => Some(*start),
            other => {
                issues.push(invalid_type("boolean", other, &[json!("start")]));
                None
            }
        };
        count.zip(start)
    });
    issues
        .into_result(parsed)
        .map(|(count, start)| (count as usize, start))
}

pub async fn fan_out_tries(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let (count, start) = match fan_out_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::CREATED, move || {
        let run = Arc::clone(&state.stages.run);
        let parts = FanOutParts {
            tasks: state.tasks.clone(),
            workspaces: Arc::clone(&state.workspaces),
            start_by_hand: Arc::new(move |task_id: &str, agent: &str| {
                run.start_by_hand(task_id, agent, None, None)
            }),
            data_dir: state.data_dir.clone(),
        };
        let tasks = fan_out(&parts, &id, count, start)?;
        Ok(json!({ "tasks": to_value(&tasks) }))
    })
    .await
}

/// What `newWorkspaceBodySchema` checks, without the attachments.
struct NewBody {
    input: NewWorkspaceInput,
    repo: Option<String>,
}

fn new_workspace_fields(body: Option<&Value>, issues: &mut Issues) -> Option<NewBody> {
    let map = object(body, &[], issues)?;
    let name = title(map.get("title"), &[json!("title")], issues);
    // Null, like none, is the default repository.
    let repo = match map.get("repo") {
        None | Some(Value::Null) => None,
        Some(repo) => string(
            Some(repo),
            &[json!("repo")],
            &natamux_core::formats::REPO_ID,
            issues,
        ),
    };
    let base = base_field(map, issues);
    let branch = branch_field(map, issues);
    // The first stage starts at once, so it must be an agent the daemon can start.
    let agent = map
        .get("agent")
        .and_then(|agent| one_of(Some(agent), &[json!("agent")], &LAUNCHABLE_AGENTS, issues));
    let prompt = map.get("prompt").and_then(|prompt| {
        string(
            Some(prompt),
            &[json!("prompt")],
            &[Rule::Trim, Rule::Max(20_000)],
            issues,
        )
    });
    let context = map
        .get("context")
        .and_then(|context| task_context(context, &[json!("context")], issues))
        .and_then(|context| serde_json::from_value::<TaskContext>(context).ok());
    let title = name?.as_str().map(str::to_string)?;
    Some(NewBody {
        repo: repo.clone(),
        input: NewWorkspaceInput {
            title,
            repo,
            base,
            branch,
            agent,
            prompt,
            context,
            attachments: Vec::new(),
        },
    })
}

fn new_workspace_body(body: Option<&Value>) -> Result<(NewBody, Vec<Upload>), ZodError> {
    let mut issues = Issues::default();
    let fields = new_workspace_fields(body, &mut issues);
    let fields = issues.into_result(fields)?;
    let uploads = attachments_body(body.unwrap_or(&Value::Null))?;
    Ok((fields, uploads))
}

/// A task, its worktree and its agent in one step. The answer comes before the worktree is made.
pub async fn start_workspace(state: Arc<AppState>, body: Option<Value>) -> Answer {
    let (fields, uploads) = match new_workspace_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::ACCEPTED, move || {
        // Before the task is made, so a missing repository or a file over a limit leaves no task.
        state.repos.get(fields.repo.as_deref())?;
        let mut input = fields.input;
        input.attachments = decode_attachments(&uploads)?;
        let (leaving, starting) = (Arc::clone(&state.stages.run), Arc::clone(&state.stages.run));
        let parts = NewWorkspaceParts {
            tasks: state.tasks.clone(),
            workspaces: Arc::clone(&state.workspaces),
            leave_to_person: Arc::new(move |id: &str| leaving.leave_to_person(id)),
            start_stage: Arc::new(move |id: &str| {
                starting.act(id, StageAction::Start, &StageActionInput::default())
            }),
        };
        let task = new_workspace(&parts, input)?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

/// `commitBodySchema`: the message, trimmed, 1 to 10000 characters.
fn commit_body(body: Option<&Value>) -> Result<String, ZodError> {
    let mut issues = Issues::default();
    let message = object(body, &[], &mut issues).and_then(|map| {
        string(
            map.get("message"),
            &[json!("message")],
            &[Rule::Trim, Rule::Min(1), Rule::Max(10_000)],
            &mut issues,
        )
    });
    issues.into_result(message)
}

pub async fn commit(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let message = match commit_body(body.as_ref()) {
        Ok(message) => message,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        let (task, hash) = state.workspaces.commit(&id, &message)?;
        Ok(json!({ "task": to_value(&task), "hash": hash }))
    })
    .await
}

/// `rollbackBodySchema`: 1 to 5000 paths of 1 to 1024 characters.
fn rollback_body(body: Option<&Value>) -> Result<Vec<String>, ZodError> {
    let mut issues = Issues::default();
    let paths = object(body, &[], &mut issues).and_then(|map| {
        let length = Length {
            min: Some(1),
            max: Some(5000),
        };
        array(
            map.get("paths"),
            &[json!("paths")],
            length,
            &mut issues,
            |each, path, issues| string(Some(each), path, &[Rule::Min(1), Rule::Max(1024)], issues),
        )
    });
    issues.into_result(paths)
}

pub async fn rollback(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let paths = match rollback_body(body.as_ref()) {
        Ok(paths) => paths,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        Ok(json!({ "files": state.workspaces.rollback(&id, &paths)? }))
    })
    .await
}

pub async fn undo_rollback(state: Arc<AppState>, id: String) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(json!({ "files": state.workspaces.undo_rollback(&id)? }))
    })
    .await
}

/// A draft of the commit message, for the user to read before the commit.
pub async fn commit_message(state: Arc<AppState>, id: String) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(to_value(&state.workspaces.draft_commit_message(&id)?))
    })
    .await
}

pub async fn push(state: Arc<AppState>, id: String) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(json!({ "task": to_value(&state.workspaces.push(&id)?) }))
    })
    .await
}

pub async fn open_pull_request(state: Arc<AppState>, id: String) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(json!({ "task": to_value(&state.workspaces.open_pull_request(&id)?) }))
    })
    .await
}

/// Asks GitHub now instead of waiting for the next minute.
pub async fn refresh_pull_request(state: Arc<AppState>, id: String) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    match tokio::task::spawn_blocking(move || state.pull_requests.check(&id)).await {
        Ok(Ok(task)) => json_answer(StatusCode::OK, &json!({ "task": to_value(&task) })),
        Ok(Err(error)) => watch_error(&error),
        Err(_) => internal_error(),
    }
}
