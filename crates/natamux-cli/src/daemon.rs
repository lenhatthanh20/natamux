//! The daemon's HTTP API as the command calls it (`packages/cli/src/client.ts`): the same
//! routes and bodies, and the same words for its errors.

use std::future::Future;

use natamux_client::{Config, Method};
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::projects::repo::{Repo, RepoList};
use natamux_proto::stages::actions::STAGE_TICKET_HEADER;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::query::encode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// One HTTP request to the daemon: the status and body of its answer, or why there is none.
/// Tests give a fake one.
pub trait Transport {
    fn request(
        &self,
        method: Method,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> impl Future<Output = Result<(u16, Vec<u8>), String>>;
}

/// The daemon at `base_url`, with the token. It never reads proxy settings.
pub struct HttpTransport {
    config: Config,
}

impl HttpTransport {
    pub fn new(base_url: &str, token: &str) -> HttpTransport {
        HttpTransport {
            config: Config {
                base_url: base_url.to_string(),
                token: token.to_string(),
            },
        }
    }
}

impl Transport for HttpTransport {
    async fn request(
        &self,
        method: Method,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>), String> {
        natamux_client::request(&self.config, method, path, headers, body)
            .await
            .map(|(status, body)| (status, body.to_vec()))
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<AttachmentUpload>>,
    /// Left out for a subtask; null is the project with the id "default".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContextBody {
    pub source: String,
    #[serde(rename = "ref")]
    pub reference: String,
}

/// A file sent with a new task, as base64.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AttachmentUpload {
    pub name: String,
    pub data: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StageActionRequest {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SendEventRequest {
    pub source: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SendEventResult {
    pub task: Task,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveWorkspaceResult {
    pub task: Task,
    pub branch_deleted: bool,
}

/// Only the fields of `TaskChanges` that were given.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskChangesBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}

fn unexpected(what: &str) -> String {
    format!("Unexpected answer from the daemon: {what}")
}

/// `String(value)` of JavaScript, for the parts of an error the daemon sent.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
        Some(other) => other.to_string(),
    }
}

fn describe_issue(issue: &Value) -> Result<String, String> {
    let Value::Object(issue) = issue else {
        return Err(unexpected("issue"));
    };
    let message = js_string(issue.get("message"));
    let place = match issue.get("path") {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|part| match part {
                Value::Null => String::new(),
                other => js_string(Some(other)),
            })
            .collect::<Vec<_>>()
            .join("."),
        _ => String::new(),
    };
    Ok(if place.is_empty() {
        message
    } else {
        format!("{place}: {message}")
    })
}

pub fn describe_error(status: u16, body: Option<&Value>) -> Result<String, String> {
    if status == 401 {
        return Ok("The daemon rejected the token. Check NATAMUX_TOKEN.".into());
    }
    if let Some(Value::Object(fields)) = body {
        if fields.get("error") == Some(&Value::String("invalid_request".into()))
            && let Some(Value::Array(issues)) = fields.get("issues")
        {
            let issues = issues
                .iter()
                .map(describe_issue)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(format!("Invalid request. {}", issues.join("; ")));
        }
        if let Some(Value::String(message)) = fields.get("message") {
            return Ok(message.clone());
        }
    }
    if status == 403 {
        return Ok(
            "Only a person can do this, not the agent token of a task terminal. Use the dashboard."
                .into(),
        );
    }
    Ok(format!("The daemon answered with status {status}."))
}

pub struct DaemonClient<T> {
    base_url: String,
    transport: T,
}

fn task_path(id: &str) -> String {
    format!("/tasks/{}", encode(id))
}

impl<T: Transport> DaemonClient<T> {
    /// `base_url` with no "/" at the end; only for messages, the transport knows it too.
    pub fn new(base_url: &str, transport: T) -> DaemonClient<T> {
        DaemonClient {
            base_url: base_url.to_string(),
            transport,
        }
    }

    pub async fn list_tasks(&self, state: Option<&str>) -> Result<Vec<Task>, String> {
        let query = state
            .map(|state| format!("?state={}", encode(state)))
            .unwrap_or_default();
        let body = self
            .call(Method::GET, &format!("/tasks{query}"), None::<&()>)
            .await?;
        field(body, "tasks", "task list")
    }

    pub async fn create_task(&self, request: &CreateTaskRequest) -> Result<Task, String> {
        let body = self.call(Method::POST, "/tasks", Some(request)).await?;
        field(body, "task", "created task")
    }

    pub async fn list_repos(&self) -> Result<Vec<Repo>, String> {
        let body = self.call(Method::GET, "/repos", None::<&()>).await?;
        let list: RepoList = parse(body, "project list")?;
        Ok(list.repos)
    }

    pub async fn get_task(&self, id: &str) -> Result<Task, String> {
        let body = self.call(Method::GET, &task_path(id), None::<&()>).await?;
        field(body, "task", "task")
    }

    pub async fn send_event(
        &self,
        id: &str,
        request: &SendEventRequest,
    ) -> Result<SendEventResult, String> {
        let path = format!("{}/events", task_path(id));
        let body = self.call(Method::POST, &path, Some(request)).await?;
        let accepted =
            body.as_ref().and_then(|body| body.get("accepted")) == Some(&Value::Bool(true));
        Ok(SendEventResult {
            task: field(body, "task", "event result")?,
            accepted,
        })
    }

    pub async fn list_events(&self, id: &str) -> Result<Vec<StoredEvent>, String> {
        let path = format!("{}/events", task_path(id));
        let body = self.call(Method::GET, &path, None::<&()>).await?;
        field(body, "events", "event log")
    }

    pub async fn update_task(&self, id: &str, changes: &TaskChangesBody) -> Result<Task, String> {
        let body = self
            .call(Method::PATCH, &task_path(id), Some(changes))
            .await?;
        field(body, "task", "edited task")
    }

    /// A task with no workspace gets one first, from `base` and `branch` when they are given.
    pub async fn start_agent(
        &self,
        id: &str,
        agent: &str,
        prompt: Option<&str>,
        base: Option<&str>,
        branch: Option<&str>,
    ) -> Result<Task, String> {
        #[derive(Serialize)]
        struct Start<'a> {
            agent: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            prompt: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            base: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            branch: Option<&'a str>,
        }
        let path = format!("{}/agent", task_path(id));
        let start = Start {
            agent,
            prompt,
            base,
            branch,
        };
        let body = self.call(Method::POST, &path, Some(&start)).await?;
        field(body, "task", "started agent")
    }

    /// `ticket`: the one-time result ticket of a command stage, which lets its script pass.
    pub async fn stage_action(
        &self,
        id: &str,
        request: &StageActionRequest,
        ticket: Option<&str>,
    ) -> Result<Task, String> {
        let path = format!("{}/stage", task_path(id));
        let extra: Vec<_> = ticket
            .map(|ticket| (STAGE_TICKET_HEADER, ticket))
            .into_iter()
            .collect();
        let body = self
            .call_with(Method::POST, &path, &extra, Some(request))
            .await?;
        field(body, "task", "task")
    }

    pub async fn set_workflow(&self, id: &str, stage: &str, on: bool) -> Result<Task, String> {
        self.set_stage_choice(id, "workflow", stage, on).await
    }

    pub async fn set_ci(&self, id: &str, stage: &str, on: bool) -> Result<Task, String> {
        self.set_stage_choice(id, "ci", stage, on).await
    }

    async fn set_stage_choice(
        &self,
        id: &str,
        what: &str,
        stage: &str,
        on: bool,
    ) -> Result<Task, String> {
        let path = format!("{}/{what}", task_path(id));
        let choice = serde_json::json!({ "stage": stage, "on": on });
        let body = self.call(Method::PUT, &path, Some(&choice)).await?;
        field(body, "task", "task")
    }

    pub async fn link_issue(
        &self,
        id: &str,
        provider: &str,
        key: &str,
        url: &str,
    ) -> Result<Task, String> {
        let path = format!("{}/issue", task_path(id));
        let link = serde_json::json!({ "provider": provider, "key": key, "url": url });
        let body = self.call(Method::PUT, &path, Some(&link)).await?;
        field(body, "task", "task")
    }

    pub async fn create_workspace(
        &self,
        id: &str,
        base: Option<&str>,
        branch: Option<&str>,
    ) -> Result<Task, String> {
        let path = format!("{}/workspace", task_path(id));
        // The daemon refuses an empty JSON body, so no body when both are unset.
        let input = (base.is_some() || branch.is_some()).then(|| {
            let mut input = serde_json::Map::new();
            if let Some(base) = base {
                input.insert("base".into(), base.into());
            }
            if let Some(branch) = branch {
                input.insert("branch".into(), branch.into());
            }
            serde_json::Value::Object(input)
        });
        let body = self.call(Method::POST, &path, input.as_ref()).await?;
        field(body, "task", "created workspace")
    }

    pub async fn remove_workspace(
        &self,
        id: &str,
        delete_branch: bool,
    ) -> Result<RemoveWorkspaceResult, String> {
        let path = format!("{}/workspace?deleteBranch={delete_branch}", task_path(id));
        let body = self.call(Method::DELETE, &path, None::<&()>).await?;
        let branch_deleted =
            body.as_ref().and_then(|body| body.get("branchDeleted")) == Some(&Value::Bool(true));
        Ok(RemoveWorkspaceResult {
            task: field(body, "task", "removed workspace")?,
            branch_deleted,
        })
    }

    /// The answer as JSON, None for an empty or unreadable body; Err with the daemon's words.
    pub(crate) async fn call<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Option<Value>, String> {
        self.call_with(method, path, &[], body).await
    }

    async fn call_with<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        extra: &[(&str, &str)],
        body: Option<&B>,
    ) -> Result<Option<Value>, String> {
        let body = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|error| error.to_string())?;
        let mut headers = extra.to_vec();
        // The daemon refuses a request that says it is JSON but has no body.
        if body.is_some() {
            headers.push(("content-type", "application/json"));
        }
        let (status, answer) = self
            .transport
            .request(method, path, &headers, body)
            .await
            .map_err(|_| {
                format!(
                    "Cannot reach the daemon at {}. Is it running?",
                    self.base_url
                )
            })?;
        let parsed = if answer.is_empty() {
            None
        } else {
            serde_json::from_slice(&answer).ok()
        };
        if !(200..300).contains(&status) {
            return Err(describe_error(status, parsed.as_ref())?);
        }
        Ok(parsed)
    }
}

fn parse<V: DeserializeOwned>(value: Option<Value>, what: &str) -> Result<V, String> {
    let value = value.ok_or_else(|| unexpected(what))?;
    serde_json::from_value(value).map_err(|_| unexpected(what))
}

/// One field of an answer that must be an object.
fn field<V: DeserializeOwned>(body: Option<Value>, name: &str, what: &str) -> Result<V, String> {
    let Some(Value::Object(mut fields)) = body else {
        return Err(unexpected(what));
    };
    parse(fields.remove(name), what)
}
