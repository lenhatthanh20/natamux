//! `natamux stage ci-approve <stage> --task <id> --tree <id>`: a person approves the exact tree a
//! CI push sends under Strict `ciPush: approved-snapshot`. The agent token is refused.

use natamux_client::Method;
use natamux_proto::tasks::task::is_task_id;
use natamux_proto::terminals::query::encode;
use serde_json::json;

use crate::args::Parsed;
use crate::daemon::{DaemonClient, Transport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiApproveCommand {
    pub task: String,
    pub stage: String,
    pub tree: String,
    pub json: bool,
}

const USAGE: &str =
    "Give the stage, --task and --tree: natamux stage ci-approve implement --task T-42 --tree <id>";

/// `rest`: the words after `stage ci-approve`.
pub fn parse(rest: &[String], values: &Parsed, json: bool) -> Result<CiApproveCommand, String> {
    if let Some(extra) = rest.get(1..).filter(|extra| !extra.is_empty()) {
        return Err(format!("Unexpected argument: {}", extra.join(" ")));
    }
    let (Some(stage), Some(task), Some(tree)) =
        (rest.first(), values.text("task"), values.text("tree"))
    else {
        return Err(USAGE.into());
    };
    if !is_task_id(task) {
        return Err(format!(
            "\"{task}\" is not a task id. A task id looks like T-42 or ABC-42"
        ));
    }
    Ok(CiApproveCommand {
        task: task.to_string(),
        stage: stage.clone(),
        tree: tree.to_string(),
        json,
    })
}

/// Gives what to print.
pub async fn run<T: Transport>(
    command: &CiApproveCommand,
    client: &DaemonClient<T>,
) -> Result<String, String> {
    let path = format!(
        "/tasks/{}/stages/{}/ci-approve",
        encode(&command.task),
        encode(&command.stage)
    );
    let body = json!({ "tree": command.tree });
    let answer = client.call(Method::POST, &path, Some(&body)).await?;
    if command.json {
        let task = answer.and_then(|body| body.get("task").cloned());
        let text = serde_json::to_string_pretty(&task).map_err(|error| error.to_string())?;
        return Ok(format!("{text}\n"));
    }
    Ok(format!(
        "Approved the CI push of {} on {} (tree {}).\n",
        command.stage, command.task, command.tree
    ))
}
