//! What each command does and prints (`packages/cli/src/execute.ts`).

use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use natamux_proto::tasks::attachments::attachment_limit_error;
use serde::Serialize;

use crate::command::{Command, NewTask, TaskChanges};
use crate::daemon::{
    AttachmentUpload, ContextBody, CreateTaskRequest, DaemonClient, SendEventRequest,
    StageActionRequest, TaskChangesBody, Transport,
};
use crate::format::{
    format_event_log, format_event_result, format_task, format_task_list, format_workspace_created,
    format_workspace_removed,
};
use crate::run::{Env, Io};

/// `JSON.stringify(data, null, 2)` and a new line.
pub fn print_json(io: &mut dyn Io, data: &impl Serialize) {
    match serde_json::to_string_pretty(data) {
        Ok(text) => io.stdout(&format!("{text}\n")),
        Err(error) => io.stderr(&format!("Error: {error}\n")),
    }
}

/// The text is built only when the command does not ask for JSON.
fn output(io: &mut dyn Io, json: bool, data: &impl Serialize, text: impl FnOnce() -> String) {
    if json {
        print_json(io, data);
    } else {
        io.stdout(&text());
    }
}

/// The terminal the command runs in, when it is a terminal of that task. Inside a group it tells
/// the daemon which stage the agent there works on.
fn terminal_of(env: &Env, task_id: &str) -> Option<String> {
    let terminal = env.trimmed("NATAMUX_TERMINAL")?;
    (env.trimmed("NATAMUX_TASK_ID") == Some(task_id)).then(|| terminal.to_string())
}

/// The project of a new task that names none: the one of the task this terminal belongs to, or
/// the only project. The daemon would pick the "default" project, which may not exist.
async fn project_of<T: Transport>(
    client: &DaemonClient<T>,
    env: &Env,
) -> Result<Option<String>, String> {
    if let Some(task_id) = env.trimmed("NATAMUX_TASK_ID") {
        return Ok(client.get_task(task_id).await?.repo);
    }
    let repos = client.list_repos().await?;
    match repos.as_slice() {
        [only] => Ok(Some(only.id.clone())),
        [] => Err("There is no project. Add a project first".into()),
        _ => Err(format!(
            "Name the project with --repo. Projects: {}",
            repos
                .iter()
                .map(|repo| repo.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Every file is checked before the first is read and before the task is made, so a missing file
/// or one over a limit makes no task.
fn read_attachments(paths: &[String]) -> Result<Vec<AttachmentUpload>, String> {
    let mut files = Vec::new();
    for path in paths {
        let size = std::fs::metadata(path)
            .ok()
            .filter(|info| info.is_file())
            .map(|info| info.len())
            .ok_or_else(|| format!("There is no file at {path}"))?;
        let name = Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        files.push((path, name, size));
    }
    let sized: Vec<(String, u64)> = files
        .iter()
        .map(|(_, name, size)| (name.clone(), *size))
        .collect();
    if let Some(error) = attachment_limit_error(&sized) {
        return Err(error);
    }
    files
        .into_iter()
        .map(|(path, name, _)| {
            let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
            Ok(AttachmentUpload {
                name,
                data: STANDARD.encode(bytes),
            })
        })
        .collect()
}

/// What each command needs to run and print.
struct Ctx<'a, T> {
    client: &'a DaemonClient<T>,
    io: &'a mut dyn Io,
    env: &'a Env,
}

pub async fn execute<T: Transport>(
    command: &Command,
    client: &DaemonClient<T>,
    io: &mut dyn Io,
    env: &Env,
) -> Result<(), String> {
    let mut cx = Ctx { client, io, env };
    let cx = &mut cx;
    match command {
        Command::Help | Command::HooksClaudeCode => Ok(()),
        Command::TaskCreate(task) => create_task(task, cx).await,
        Command::TaskList { state, json } => list_tasks(state.as_deref(), *json, cx).await,
        Command::TaskShow { id, json } => {
            show_task(id.as_deref().unwrap_or_default(), *json, cx).await
        }
        Command::EventSend {
            id,
            kind,
            source,
            payload,
            json,
        } => {
            let request = SendEventRequest {
                source: source.clone(),
                kind: kind.clone(),
                payload: payload.clone(),
            };
            send_event(id, &request, *json, cx).await
        }
        Command::EventList { id, json } => list_events(id, *json, cx).await,
        Command::TaskEdit { id, changes, json } => edit_task(id, changes, *json, cx).await,
        Command::TaskLink {
            id,
            provider,
            key,
            url,
            json,
        } => link_issue(id, [provider, key, url], *json, cx).await,
        Command::WorkspaceCreate { id, base, json } => {
            create_workspace(id, base.as_deref(), *json, cx).await
        }
        Command::WorkspaceRemove {
            id,
            delete_branch,
            json,
        } => remove_workspace(id, *delete_branch, *json, cx).await,
        _ => agent_or_stage(command, cx).await,
    }
}

/// The commands of agents and stages; `execute` runs the others.
async fn agent_or_stage<T: Transport>(
    command: &Command,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    match command {
        Command::AgentStart {
            id,
            agent,
            prompt,
            base,
            json,
        } => start_agent(id, agent, prompt.as_deref(), base.as_deref(), *json, cx).await,
        Command::AgentDone { id, stage, json } => {
            report_done(id.as_deref().unwrap_or_default(), stage, *json, cx).await
        }
        Command::StageAct {
            id,
            action,
            stage,
            to,
            reason,
            json,
        } => {
            let request = StageActionRequest {
                action: if action == "back" { "reopen" } else { action }.to_string(),
                stage: stage.clone(),
                terminal: terminal_of(cx.env, id),
                to: to.clone(),
                reason: reason.clone(),
            };
            act_on_stage(id, &request, *json, cx).await
        }
        Command::StageWorkflow {
            id,
            stage,
            on,
            json,
        } => set_workflow(id, stage, *on, *json, cx).await,
        Command::StageCi {
            id,
            stage,
            on,
            json,
        } => set_ci(id, stage, *on, *json, cx).await,
        Command::AgentAsk {
            id,
            question,
            stage,
            json,
        } => {
            ask_question(
                id.as_deref().unwrap_or_default(),
                question,
                stage,
                *json,
                cx,
            )
            .await
        }
        _ => Ok(()),
    }
}

async fn create_task<T: Transport>(task: &NewTask, cx: &mut Ctx<'_, T>) -> Result<(), String> {
    let attachments = read_attachments(task.attach.as_deref().unwrap_or_default())?;
    // A subtask has no repo of its own unless named: the daemon uses the parent's project.
    let repo = match (&task.repo, &task.parent_id) {
        (Some(repo), _) => Some(Some(repo.clone())),
        (None, None) => Some(project_of(cx.client, cx.env).await?),
        (None, Some(_)) => None,
    };
    let request = CreateTaskRequest {
        title: task.title.clone(),
        description: task.description.clone(),
        priority: task.priority.clone(),
        labels: task.labels.clone(),
        agent: task.agent.clone(),
        depends_on: task.depends_on.clone(),
        context: task.context.as_ref().map(|context| ContextBody {
            source: context.source.clone(),
            reference: context.reference.clone(),
        }),
        attachments: (!attachments.is_empty()).then_some(attachments),
        repo,
        parent_id: task.parent_id.clone(),
    };
    let created = cx.client.create_task(&request).await?;
    output(cx.io, task.json, &created, || {
        format!("Created {}  {}\n", created.id, created.title)
    });
    Ok(())
}

async fn list_tasks<T: Transport>(
    state: Option<&str>,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let tasks = cx.client.list_tasks(state).await?;
    output(cx.io, json, &tasks, || {
        format!("{}\n", format_task_list(&tasks))
    });
    Ok(())
}

async fn show_task<T: Transport>(id: &str, json: bool, cx: &mut Ctx<'_, T>) -> Result<(), String> {
    let task = cx.client.get_task(id).await?;
    output(cx.io, json, &task, || format!("{}\n", format_task(&task)));
    Ok(())
}

async fn send_event<T: Transport>(
    id: &str,
    request: &SendEventRequest,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let result = cx.client.send_event(id, request).await?;
    output(cx.io, json, &result, || {
        format!("{}\n", format_event_result(&result))
    });
    Ok(())
}

async fn list_events<T: Transport>(
    id: &str,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let events = cx.client.list_events(id).await?;
    output(cx.io, json, &events, || {
        format!("{}\n", format_event_log(&events))
    });
    Ok(())
}

async fn edit_task<T: Transport>(
    id: &str,
    changes: &TaskChanges,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let body = TaskChangesBody {
        title: changes.title.clone(),
        description: changes.description.clone(),
        priority: changes.priority.clone(),
        labels: changes.labels.clone(),
        depends_on: changes.depends_on.clone(),
    };
    let task = cx.client.update_task(id, &body).await?;
    output(cx.io, json, &task, || format!("{}\n", format_task(&task)));
    Ok(())
}

async fn start_agent<T: Transport>(
    id: &str,
    agent: &str,
    prompt: Option<&str>,
    base: Option<&str>,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let task = cx.client.start_agent(id, agent, prompt, base).await?;
    output(cx.io, json, &task, || {
        format!("Started {agent} in the terminal of {}\n", task.id)
    });
    Ok(())
}

async fn report_done<T: Transport>(
    id: &str,
    stage: &Option<String>,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let request = StageActionRequest {
        action: "report".into(),
        stage: stage.clone(),
        terminal: terminal_of(cx.env, id),
        ..StageActionRequest::default()
    };
    let task = cx.client.stage_action(id, &request).await?;
    output(cx.io, json, &task, || {
        format!(
            "Reported done; the checks run now.\n{}\n",
            format_task(&task)
        )
    });
    Ok(())
}

async fn act_on_stage<T: Transport>(
    id: &str,
    request: &StageActionRequest,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let task = cx.client.stage_action(id, request).await?;
    output(cx.io, json, &task, || format!("{}\n", format_task(&task)));
    Ok(())
}

async fn set_workflow<T: Transport>(
    id: &str,
    stage: &str,
    on: bool,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let task = cx.client.set_workflow(id, stage, on).await?;
    output(cx.io, json, &task, || {
        let how = if on { "with" } else { "without" };
        format!(
            "{stage} runs {how} a workflow the next time it starts.\n{}\n",
            format_task(&task)
        )
    });
    Ok(())
}

async fn set_ci<T: Transport>(
    id: &str,
    stage: &str,
    on: bool,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let task = cx.client.set_ci(id, stage, on).await?;
    output(cx.io, json, &task, || {
        let how = if on {
            "waits for CI"
        } else {
            "does not wait for CI"
        };
        format!("{stage} {how} before it passes.\n{}\n", format_task(&task))
    });
    Ok(())
}

/// `link` is the provider, the key and the address of the issue.
async fn link_issue<T: Transport>(
    id: &str,
    [provider, key, url]: [&String; 3],
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let task = cx.client.link_issue(id, provider, key, url).await?;
    output(cx.io, json, &task, || format!("{}\n", format_task(&task)));
    Ok(())
}

async fn ask_question<T: Transport>(
    id: &str,
    question: &str,
    stage: &Option<String>,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let mut payload = serde_json::Map::new();
    payload.insert("question".into(), question.into());
    if let Some(stage) = stage {
        payload.insert("stage".into(), stage.clone().into());
    }
    if let Some(terminal) = terminal_of(cx.env, id) {
        payload.insert("terminal".into(), terminal.into());
    }
    let request = SendEventRequest {
        source: "agent".into(),
        kind: "agent.asked".into(),
        payload,
    };
    send_event(id, &request, json, cx).await
}

async fn create_workspace<T: Transport>(
    id: &str,
    base: Option<&str>,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let task = cx.client.create_workspace(id, base).await?;
    output(cx.io, json, &task, || {
        format!("{}\n", format_workspace_created(&task))
    });
    Ok(())
}

async fn remove_workspace<T: Transport>(
    id: &str,
    delete_branch: bool,
    json: bool,
    cx: &mut Ctx<'_, T>,
) -> Result<(), String> {
    let result = cx.client.remove_workspace(id, delete_branch).await?;
    output(cx.io, json, &result, || {
        format!("{}\n", format_workspace_removed(&result))
    });
    Ok(())
}
