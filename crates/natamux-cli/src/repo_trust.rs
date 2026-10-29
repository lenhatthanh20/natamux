//! `natamux repo trust <repo> [--approve <hash>]`: the commands a project runs on this machine,
//! and their approval, for Docker and headless use where there is no app to ask.

use natamux_client::Method;
use natamux_proto::projects::repo::is_repo_id;
use natamux_proto::projects::repo_trust::{ApproveRepo, RepoTrustView};
use natamux_proto::terminals::query::encode;

use crate::args::Parsed;
use crate::daemon::{DaemonClient, Transport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoTrustCommand {
    pub repo: String,
    /// The hash the user read in the list; None only shows the list.
    pub approve: Option<String>,
    pub json: bool,
}

/// `second`: the word after `repo`; `rest`: the words after it.
pub fn parse(
    second: Option<&str>,
    rest: &[String],
    values: &Parsed,
    json: bool,
) -> Result<RepoTrustCommand, String> {
    if second != Some("trust") {
        return Err(format!(
            "Unknown repo command \"{}\". Use trust",
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
        json,
    })
}

/// Indents each line of a command or script, so its end is plain to see.
fn indented(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>()
}

/// The list as the user reads it before an approval.
fn describe(repo: &str, view: &RepoTrustView) -> String {
    let state = if view.trusted {
        "approved"
    } else {
        "not approved"
    };
    let mut text = format!("Commands of {repo}: {state}\nHash: {}\n", view.hash);
    if view.commands.is_empty() && view.setup.is_empty() {
        text.push_str("\nThis project runs no commands of its own.\n");
    }
    for each in &view.commands {
        text.push_str(&format!("\n{}\n", each.place));
        text.push_str(&indented(&each.text));
    }
    for each in &view.setup {
        text.push_str(&format!("\nSetup script {}\n", each.file));
        text.push_str(&indented(&each.text));
    }
    if !view.trusted {
        text.push_str(&format!(
            "\nApprove them: natamux repo trust {repo} --approve {}\n",
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
    let path = format!("/repos/{}/trust", encode(&command.repo));
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
        None => describe(&command.repo, &view),
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
            hash: "ab12".into(),
            commands: vec![RepoCommand {
                place: "Stage test: check".into(),
                text: "pnpm test".into(),
            }],
            setup: vec![SetupScript {
                file: ".natamux-setup".into(),
                text: "pnpm install\ncp .env.example .env\n".into(),
            }],
        };
        assert_eq!(
            describe("my-app", &view),
            "Commands of my-app: not approved\nHash: ab12\n\
             \nStage test: check\n    pnpm test\n\
             \nSetup script .natamux-setup\n    pnpm install\n    cp .env.example .env\n\
             \nApprove them: natamux repo trust my-app --approve ab12\n"
        );
    }
}
