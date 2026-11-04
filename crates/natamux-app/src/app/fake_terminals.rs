//! The fake's terminals and the workspace writes of screen 2: a scripted screen per task and
//! terminal, the ended terminals of a finished task, and Create workspace, Push, Create PR and
//! Reopen agent, which change the fake data as the daemon would.

use futures::channel::mpsc::UnboundedReceiver;
use natamux_client::{DashboardEvent, TerminalEvent, TerminalStatus};
use natamux_proto::events::kind::PreparingMessage;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::ended::EndedShell;
use natamux_proto::terminals::list::TerminalLane;
use natamux_proto::workspaces::workspace::{PreparingStep, PushStatus};

use super::{Answer, FakeDaemon, events, not_found};

/// An agent's screen, with colours, wide lines and Vietnamese text.
const SCREEN: &str = concat!(
    "\x1b[1;38;5;75m● Natamux agent\x1b[0m \x1b[2m· claude-code · implement\x1b[0m\r\n\r\n",
    "\x1b[38;5;245m> Read crates/natamux-client/src/terminals/session.rs\x1b[0m\r\n",
    "\x1b[38;5;245m> Read crates/natamux-client/src/terminals/protocol.rs\x1b[0m\r\n\r\n",
    "The terminal socket resumes from the last offset the screen parsed. A snapshot\r\n",
    "comes only when the ring no longer has that offset.\r\n\r\n",
    "\x1b[32m✓\x1b[0m cargo test -p natamux-client   \x1b[32m44 passed\x1b[0m\r\n",
    "\x1b[33m!\x1b[0m cargo deny check               \x1b[33m15 advisories ignored, with reasons\x1b[0m\r\n",
    "\x1b[31m✗\x1b[0m live-daemon                    \x1b[31mwaiting for the image of main\x1b[0m\r\n\r\n",
    "Đã sửa xong lỗi kết nối lại; gõ tiếng Việt: người, được, khuỷu, thuở.\r\n\r\n",
    "\x1b[1m❯\x1b[0m ",
);

/// A stage that is done in its own terminal.
const DONE_STAGE: &str = concat!(
    "\x1b[2m> Task: Read .natamux-task/spec.md and write the cases of each item.\x1b[0m\r\n\r\n",
    "• \x1b[1mRead\x1b[0m .natamux-task/spec.md\r\n",
    "• \x1b[1mWrite\x1b[0m .natamux-task/test-spec.md\r\n",
    "  \x1b[32m✓ No problems found\x1b[0m\r\n\r\n",
    "Done with this stage.\r\n\r\n",
    "• \x1b[1mBash\x1b[0m natamux done\r\n",
    "\x1b[2m(waiting for the next prompt)\x1b[0m\r\n",
);

/// The review task's Terminal 1, with "src" five times for the find shot.
const REVIEW: &str = concat!(
    "\x1b[2m> Task: Review the changes against the spec and the code map.\x1b[0m\r\n\r\n",
    "• \x1b[1mRead\x1b[0m src/app/window.rs\r\n",
    "• \x1b[1mRead\x1b[0m src/terminals/shell_panes.rs\r\n",
    "• \x1b[1mRead\x1b[0m src/workspaces/workspace_view.rs\r\n",
    "• \x1b[1mBash\x1b[0m git diff dev...HEAD --stat -- src\r\n",
    "• \x1b[1mWrite\x1b[0m .natamux-task/review.md\r\n",
    "  \x1b[32m✓ 2 findings fixed in src/workspaces\x1b[0m\r\n\r\n",
    "Done with this stage.\r\n\r\n",
    "• \x1b[1mBash\x1b[0m natamux done\r\n",
    "\x1b[2m(waiting for the next prompt)\x1b[0m\r\n",
);

/// An agent that asks the user.
const ASKS: &str = concat!(
    "\x1b[1;38;5;209m● Claude Code\x1b[0m \x1b[2mT-0141 · natamux/t-0141-task-usage\x1b[0m\r\n\r\n",
    "\x1b[2m> Task: Count tokens and cost of each task. Read .natamux-task/context.md.\x1b[0m\r\n\r\n",
    "\x1b[33m?\x1b[0m Show the cost in USD, or only the tokens, when a model has no list price?\r\n",
);

/// A second agent of a side by side group, still at work.
const WORKING: &str = concat!(
    "\x1b[2m> Task: Write the docs of the workspace view.\x1b[0m\r\n\r\n",
    "• \x1b[1mRead\x1b[0m docs/rust-phase3.md\r\n",
    "• \x1b[1mEdit\x1b[0m docs/rust-phase3.md\r\n",
    "  \x1b[32m+18 lines\x1b[0m \x1b[31m-2 lines\x1b[0m\r\n",
);

fn script(task_id: &str, terminal: &str) -> &'static str {
    match (task_id, terminal) {
        ("T-0141", _) => ASKS,
        ("T-0143", "1") => REVIEW,
        ("T-0143", _) | ("T-0146", "10") => DONE_STAGE,
        ("T-0146", "12") => WORKING,
        _ => SCREEN,
    }
}

impl FakeDaemon {
    /// The screen of one terminal: open at once, with its script.
    pub fn terminal(&self, task_id: &str, terminal: &str) -> UnboundedReceiver<TerminalEvent> {
        let key = (task_id.to_owned(), terminal.to_owned());
        let own = self.data().screens.get(&key).cloned();
        let screen = own.unwrap_or_else(|| script(task_id, terminal).to_owned());
        events([
            TerminalEvent::Status(TerminalStatus::Open),
            TerminalEvent::Output {
                bytes: screen.as_bytes().to_vec(),
                end: Some(screen.len() as u64),
            },
        ])
    }

    /// A terminal `--shot` state: the terminal shows `screen` instead of its script.
    pub fn set_screen(&self, task_id: &str, terminal: &str, screen: String) {
        let key = (task_id.to_owned(), terminal.to_owned());
        self.data().screens.insert(key, screen);
    }

    /// The `workspace-preparing` shot: the daemon is at this step of the task's new workspace.
    pub fn set_preparing(&self, task_id: &str, step: PreparingStep) {
        self.data().preparing.insert(task_id.into(), step);
    }

    #[cfg(test)]
    pub fn tasks(&self) -> Vec<Task> {
        self.data().tasks.clone()
    }

    pub fn ended_shells(&self, task_id: &str) -> Answer<Vec<EndedShell>> {
        Ok(self.data().ended.get(task_id).cloned().unwrap_or_default())
    }

    /// The workspace is ready at once; its steps still go out, as for a new task.
    pub fn create_workspace(&self, task_id: &str, base: Option<&str>) -> Answer<Task> {
        let mut data = self.data();
        let folder = data.worktrees_dir.clone().unwrap_or_else(|| "/work".into());
        let task = data.task_mut(task_id)?;
        task.workspace = Some(format!("{folder}/{task_id}"));
        task.branch = Some(format!("natamux/{}", task_id.to_lowercase()));
        let task = task.clone();
        if let Some(base) = base {
            data.graphs.entry(task_id.into()).or_default().base = Some(base.into());
        }
        data.tasks_changed();
        for step in [
            Some(PreparingStep::Fetch),
            Some(PreparingStep::Checkout),
            None,
        ] {
            data.send(DashboardEvent::WorkspaceStep(PreparingMessage {
                task_id: task_id.into(),
                step,
                error: None,
            }));
        }
        Ok(task)
    }

    /// Origin has every commit afterwards.
    pub fn push(&self, task_id: &str) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?.clone();
        if task.branch.is_none() {
            return Err(not_found("Branch of", task_id));
        }
        let graph = data.graphs.entry(task_id.into()).or_default();
        graph.push = Some(PushStatus::Tracked {
            unpushed: 0,
            unpulled: 0,
        });
        data.send(DashboardEvent::WorkspaceChanged {
            task_id: task_id.into(),
        });
        Ok(task)
    }

    pub fn open_pull_request(&self, task_id: &str) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        let number = task_id.trim_start_matches("T-").trim_start_matches('0');
        task.pr = Some(format!(
            "https://github.com/lenhatthanh20/natamux/pull/{number}"
        ));
        let task = task.clone();
        data.tasks_changed();
        Ok(task)
    }

    /// The agent comes back in Terminal 1 and stays "restoring", as on a daemon that waits for
    /// the pane to attach.
    pub fn reopen_agent(&self, task_id: &str) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?.clone();
        let agent = task.agent.clone();
        let list = data.terminals.entry(task_id.into()).or_default();
        if !list.terminals.iter().any(|name| name == "1") {
            list.terminals.insert(0, "1".into());
        }
        if !list.agents.iter().any(|name| name == "1") {
            list.agents.insert(0, "1".into());
        }
        list.lanes.insert(
            "1".into(),
            TerminalLane {
                stage: Some(task.stage.clone()),
                state: Some(task.state),
                agent: Some(agent.clone()),
            },
        );
        list.restoring.insert("1".into(), agent);
        let event = DashboardEvent::Terminals {
            task_id: task_id.into(),
            terminals: list.clone(),
        };
        data.send(event);
        data.send(DashboardEvent::ShellsChanged {
            task_id: task_id.into(),
        });
        Ok(task)
    }
}
