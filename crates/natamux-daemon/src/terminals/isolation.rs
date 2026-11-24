//! Execution isolation (`isolation.mode: required` in `security.json`, read at start): every task
//! shell, stage check command and commit-draft agent runs under Landlock with only its own
//! worktree, its private tmp and Claude Code's files in reach, or it does not run. Also the
//! spawn wrapper that writes the agents' hook files at each shell start.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use natamux_agents::hook_files::{WorkspaceHooks, prepare_agent_hooks};
use natamux_agents::launch_isolation::{ISOLATION_AGENT_REASON, isolated_adapter_ok};
use natamux_core::formats::is_task_id_shape;
use natamux_core::security::{IsolationSettings, install_git_hardening, load_isolation, network_for};
use natamux_core::workspaces::private_dir::{private_dir, write_private};
use natamux_proto::projects::security::IsolationView;
use natamux_pty::sandbox_plan::{isolate_command, landlock_abi};
use natamux_pty::{Env, SpawnOptions};

use super::isolation_network::tcp_ports;
use super::isolation_plan::{PlanInput, isolated_env, plan_of};
use super::login_path::use_login_path;
use super::session_lifecycle::{SpawnPty, random_id};

/// `<data>/sandbox/<task>`: launch scripts, the git config and `tmp` of an isolated task.
pub const SANDBOX_DIR: &str = "sandbox";
/// Marks every process an isolated shell starts, for the sweep after a kill.
pub const SANDBOX_MARK: &str = "NATAMUX_SANDBOX";

pub struct Isolation {
    pub settings: IsolationSettings,
    pub data_dir: PathBuf,
    pub bin_dir: Option<String>,
    pub home: PathBuf,
    /// The Landlock ABI, or why isolation cannot run on this machine.
    pub support: Result<u32, String>,
    /// `user.name` and `user.email` for git in the shells: the user's own git config is out of
    /// reach.
    pub git_user: Option<(String, String)>,
}

fn global_git(key: &str) -> Option<String> {
    let value = natamux_git::exec::git(Path::new("."), &["config", "--global", key], None, &[]);
    value
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn unsupported(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, reason.to_string())
}

/// A value inside double quotes in a git config file.
fn git_quoted(value: &str) -> String {
    let clean: String = value.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", clean.replace('\\', "\\\\").replace('"', "\\\""))
}

impl Isolation {
    pub fn new(
        settings: IsolationSettings,
        data_dir: &Path,
        bin_dir: Option<String>,
        home: &Path,
    ) -> Isolation {
        let git_user = global_git("user.name").zip(global_git("user.email"));
        Isolation {
            settings,
            data_dir: data_dir.to_path_buf(),
            bin_dir,
            home: home.to_path_buf(),
            support: landlock_abi(),
            git_user,
        }
    }

    /// The Landlock ABI, or the reason no isolated process can start.
    pub fn ready(&self) -> io::Result<u32> {
        self.support.clone().map_err(|reason| unsupported(&reason))
    }

    /// Whether `agent` may start: only Claude Code, and only where isolation can run.
    pub fn allows_agent(&self, agent: &str) -> Result<(), String> {
        self.ready().map_err(|error| error.to_string())?;
        if !isolated_adapter_ok(agent) {
            return Err(ISOLATION_AGENT_REASON.to_string());
        }
        Ok(())
    }

    pub fn task_dir(&self, task_id: &str) -> PathBuf {
        self.data_dir.join(SANDBOX_DIR).join(task_id)
    }

    /// Makes the task's private folders and git config. Gives the task folder and its tmp.
    fn prepare_task(&self, task_id: &str) -> io::Result<(PathBuf, PathBuf)> {
        if !is_task_id_shape(task_id) {
            return Err(unsupported(&format!(
                "No task id for isolation: {task_id:?}"
            )));
        }
        let task_dir = self.task_dir(task_id);
        let tmp = task_dir.join("tmp");
        private_dir(&tmp)?;
        let mut config = String::from("[credential]\n\thelper =\n");
        if let Some((name, email)) = &self.git_user {
            config.push_str(&format!(
                "[user]\n\tname = {}\n\temail = {}\n",
                git_quoted(name),
                git_quoted(email)
            ));
        }
        write_private(&task_dir.join("gitconfig"), config.as_bytes())?;
        Ok((task_dir, tmp))
    }

    fn env_for(&self, env: &Env, task_dir: &Path, tmp: &Path) -> Env {
        let mut env = isolated_env(env, &self.settings.env_allow);
        let text = |path: &Path| path.to_string_lossy().into_owned();
        env.insert("TMPDIR".into(), text(tmp));
        env.insert(
            "GIT_CONFIG_GLOBAL".into(),
            text(&task_dir.join("gitconfig")),
        );
        env
    }

    fn input<'a>(&'a self, worktree: &'a Path, dirs: &'a (PathBuf, PathBuf)) -> PlanInput<'a> {
        PlanInput {
            worktree,
            task_dir: &dirs.0,
            tmp: &dirs.1,
            home: &self.home,
            data_dir: &self.data_dir,
            bin_dir: self.bin_dir.as_deref(),
            path: None,
            settings: &self.settings,
            sweep_mark: None,
        }
    }

    /// A task shell: the task comes from `NATAMUX_TASK_ID`, the worktree is its folder.
    pub fn isolate_shell(&self, options: &mut SpawnOptions) -> io::Result<()> {
        self.ready()?;
        let task_id = options
            .env
            .get("NATAMUX_TASK_ID")
            .cloned()
            .unwrap_or_default();
        let dirs = self.prepare_task(&task_id)?;
        let mut env = self.env_for(&options.env, &dirs.0, &dirs.1);
        let mark = random_id(12);
        env.insert(SANDBOX_MARK.into(), mark.clone());
        let mut plan = plan_of(PlanInput {
            path: env.get("PATH").map(String::as_str),
            sweep_mark: Some(format!("{SANDBOX_MARK}={mark}")),
            ..self.input(&options.cwd, &dirs)
        });
        plan.tcp_connect = tcp_ports(&env, &network_for(&options.cwd));
        options.env = env;
        options.isolation = Some(plan);
        Ok(())
    }

    /// A command the daemon runs in a worktree (a stage check, the commit-draft agent). Natamux
    /// names a worktree after its task. It has no task token: the daemon's own is dropped.
    pub fn isolate_command(&self, command: &mut Command, worktree: &Path) -> io::Result<()> {
        self.ready()?;
        let task_id = worktree
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dirs = self.prepare_task(&task_id)?;
        let mut env = self.env_for(&env_of(command), &dirs.0, &dirs.1);
        env.remove("NATAMUX_TOKEN");
        let mut plan = plan_of(PlanInput {
            path: env.get("PATH").map(String::as_str),
            ..self.input(worktree, &dirs)
        });
        plan.tcp_connect = tcp_ports(&env, &network_for(worktree));
        command.env_clear().envs(&env);
        isolate_command(command, &plan)
    }

    /// `isolation` of `GET /security` while the daemon runs with isolation required.
    pub fn view(&self) -> IsolationView {
        view_of(true, &self.support)
    }
}

/// The environment a command would start with: the daemon's, changed by what was set on it.
fn env_of(command: &Command) -> Env {
    let mut env: Env = std::env::vars().collect();
    for (name, value) in command.get_envs() {
        let name = name.to_string_lossy().into_owned();
        match value {
            Some(value) => env.insert(name, value.to_string_lossy().into_owned()),
            None => env.remove(&name),
        };
    }
    env
}

pub fn view_of(required: bool, support: &Result<u32, String>) -> IsolationView {
    IsolationView {
        mode: String::from(if required { "required" } else { "off" }),
        backend: String::from(if cfg!(target_os = "linux") {
            "landlock"
        } else {
            "none"
        }),
        abi: support.as_ref().ok().copied(),
        ok: required && support.is_ok(),
        reason: support.as_ref().err().cloned(),
    }
}

/// `isolation` of `GET /security`.
pub fn isolation_body(isolation: Option<&Arc<Isolation>>) -> IsolationView {
    isolation.map_or_else(
        || view_of(false, &landlock_abi()),
        |isolation| isolation.view(),
    )
}

/// The isolation of a starting daemon, None when off. Turns on the hardened daemon git too.
pub fn start_isolation(
    data_dir: &Path,
    bin_dir: Option<String>,
    home: &Path,
) -> Option<Arc<Isolation>> {
    let settings = load_isolation(data_dir);
    if !settings.required {
        return None;
    }
    let isolation = Isolation::new(settings, data_dir, bin_dir, home);
    let no_hooks = data_dir.join(SANDBOX_DIR).join("no-hooks");
    match private_dir(&no_hooks) {
        Ok(()) => install_git_hardening(no_hooks),
        Err(error) => eprintln!("The hardened git has no empty hook folder: {error}"),
    }
    if let Err(reason) = &isolation.support {
        eprintln!("Isolation is required but cannot run, so no task shell starts: {reason}");
    }
    Some(Arc::new(isolation))
}

/// Isolates every shell when isolation is on; a shell that cannot be isolated does not start.
pub fn with_isolation(spawn: Arc<SpawnPty>, isolation: Option<Arc<Isolation>>) -> Arc<SpawnPty> {
    let Some(isolation) = isolation else {
        return spawn;
    };
    Arc::new(move |mut options: SpawnOptions| {
        isolation.isolate_shell(&mut options)?;
        (*spawn)(options)
    })
}

/// Takes the user's git and GitHub credentials out of a shell's environment when the network
/// settings say `strip`, and sets the proxy. Read at each shell start, so a change applies to
/// the next shell.
pub fn with_child_env(spawn: Arc<SpawnPty>) -> Arc<SpawnPty> {
    Arc::new(move |mut options: SpawnOptions| {
        options.env = natamux_core::child_env::scrub(&options.env);
        (*spawn)(options)
    })
}

/// An agent the user starts by hand in a shell finds its hooks there: the files are written at
/// each shell start, and only there, not on the output path.
pub fn with_hook_files(spawn: Arc<SpawnPty>, bin_dir: Option<String>) -> Arc<SpawnPty> {
    Arc::new(move |mut options: SpawnOptions| {
        // The shell is the program that needs the login PATH; the daemon itself does not wait.
        use_login_path(&mut options.env, bin_dir.as_deref());
        prepare_agent_hooks(&options.cwd, WorkspaceHooks { claude_code: true });
        (*spawn)(options)
    })
}

#[cfg(test)]
#[path = "isolation_tests.rs"]
mod tests;
