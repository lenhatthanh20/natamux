//! T-0373 Part 1 (F2): the launch gate in Strict mode. A worktree that is not approved as it is
//! now starts no agent on any path, and an approved start asks before each action.

use natamux_agents::hook_files::exclude_from_git;
use natamux_agents::launch::LAST_SESSION;
use natamux_agents::trust::{TrustPaths, trust_workspace};
use natamux_core::security::{Forced, SecurityMode, StrictSettings, force};
use natamux_daemon::projects::repo_trust_worktree::{TrustState, TrustedOf, trust_always};
use natamux_git::test_support::{run_git, test_repo};

use super::*;

fn strict() -> Forced {
    force(SecurityMode::Strict(StrictSettings::default()))
}

fn runner_with(env: &Env, trusted_of: TrustedOf) -> AgentRunner {
    AgentRunner::new(AgentRunnerOptions {
        trusted_of,
        ..env.options()
    })
}

/// Trust markers of Cursor and Copilot for the env's worktree, as an earlier start left them.
fn old_markers(env: &Env) -> (TrustPaths, PathBuf) {
    let paths = trust_paths(|_| None, &env.data_dir);
    std::fs::create_dir_all(paths.copilot_config.parent().unwrap()).unwrap();
    std::fs::write(&paths.copilot_config, "{}").unwrap();
    trust_workspace("cursor", &env.workspace, &paths);
    trust_workspace("copilot", &env.workspace, &paths);
    let slug = std::fs::read_dir(&paths.cursor_projects)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let marker = slug.join(".workspace-trusted");
    assert!(marker.exists());
    (paths, marker)
}

/// The five ways an agent starts: new launch, resume, restore after a restart, a paused shell
/// reused, reopen.
fn starts() -> Vec<(
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    OwnAgent,
)> {
    let restored = OwnAgent {
        restored: true,
        ..OwnAgent::default()
    };
    vec![
        ("new", None, None, OwnAgent::default()),
        ("resume", None, Some("abc-123"), OwnAgent::default()),
        ("restore", None, Some("abc-123"), restored),
        ("paused reuse", Some("3"), None, OwnAgent::default()),
        ("reopen", None, Some(LAST_SESSION), OwnAgent::default()),
    ]
}

#[test]
fn a_changed_worktree_starts_nothing_on_any_start_path_and_takes_back_old_trust() {
    let env = Env::new();
    let (paths, marker) = old_markers(&env);
    let runner = runner_with(&env, trust_always(false));
    let task = env.task();
    let _strict = strict();

    for (name, terminal, session, own) in starts() {
        let result = runner.start(&task.id, "claude-code", None, terminal, session, &own);
        assert!(matches!(result, Err(AgentError::Untrusted(_))), "{name}");
    }

    assert_eq!(env.pty_count(), 0, "no terminal ran");
    assert!(!env.script(&task.id).exists(), "no launch script");
    assert!(!env.data_dir.join("agent-launch-T-0001-3.sh").exists());
    assert!(!marker.exists(), "the old Cursor marker is gone");
    let copilot = std::fs::read_to_string(&paths.copilot_config).unwrap();
    assert!(!copilot.contains(&env.workspace), "{copilot}");
}

#[test]
fn personal_mode_starts_a_changed_worktree_limited_as_before() {
    let env = Env::new();
    let runner = runner_with(&env, trust_always(false));
    let task = env.task();
    runner
        .start(
            &task.id,
            "claude-code",
            Some("Go"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
    assert_eq!(env.pty_count(), 1);
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' 'Go'\n"
    );
}

#[test]
fn strict_refuses_an_agent_that_cannot_be_held_to_asking() {
    let env = Env::new();
    let task = env.task();
    let _strict = strict();
    for agent in ["cursor", "copilot", "aider"] {
        let result = env
            .runner
            .start(&task.id, agent, None, None, None, &OwnAgent::default());
        assert!(matches!(result, Err(AgentError::Untrusted(_))), "{agent}");
    }
    assert_eq!(env.pty_count(), 0);
}

#[test]
fn an_approved_strict_start_asks_and_checks_the_digest_before_the_agent() {
    let env = Env::new();
    let task = env.task();
    let _strict = strict();
    let verify = "natamux repo verify-launch --task 'T-0001' --digest 'always' && ";

    env.start(&task, "claude-code", "Go");
    let line = "claude --settings '/data/claude code.json' --permission-mode default 'Go'\n";
    assert_eq!(env.script_text(&task.id), format!("{verify}{line}"));

    env.start(&task, "codex", "Go");
    let codex = env.script_text(&task.id);
    assert!(codex.starts_with(verify), "{codex}");
    assert!(
        codex.contains(" --ask-for-approval on-request --sandbox workspace-write"),
        "{codex}"
    );
    assert!(!codex.contains("never"), "{codex}");
    assert!(codex.contains("--dangerously-bypass-hook-trust"), "{codex}");

    env.start(&task, "goose", "Go");
    let goose = env.script_text(&task.id);
    assert!(
        goose.starts_with(&format!("{verify}GOOSE_MODE=approve goose session")),
        "{goose}"
    );
}

#[test]
fn the_windows_line_runs_the_agent_only_after_the_check_passed() {
    let env = Env::new();
    let task = env.task();
    let (windows, ptys) = env.windows("C:\\data\\settings.json", None);
    let _strict = strict();
    windows
        .start(
            &task.id,
            "claude-code",
            Some("Go"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
    let typed = ptys.lock().unwrap()[0].written().join("");
    // T-0375 R1-4: `$LASTEXITCODE` keeps its old value when `natamux` is not found.
    let check = "; $global:LASTEXITCODE = 1; \
                 natamux repo verify-launch --task 'T-0001' --digest 'always'; \
                 if ($? -and $LASTEXITCODE -eq 0) { claude ";
    assert!(typed.contains(check), "{typed}");
}

// T-0375 R1-4: the check runs the app's `natamux`, as on POSIX.
#[test]
fn the_windows_check_puts_the_bin_folder_first_on_the_path() {
    let env = Env::new();
    let task = env.task();
    let (windows, ptys) = env.windows("C:\\data\\settings.json", Some("C:\\Nata'mux\\bin"));
    let _strict = strict();
    let own = OwnAgent::default();
    windows
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();
    let typed = ptys.lock().unwrap()[0].written().join("");
    let check = "$global:LASTEXITCODE = 1; $env:PATH = 'C:\\Nata''mux\\bin;' + $env:PATH; \
                 natamux repo verify-launch";
    assert!(typed.contains(check), "{typed}");
}

/// Gives the approved digest for the first `calls` asks, then `later`.
fn turns_after(calls: usize, later: TrustState) -> TrustedOf {
    let count = Arc::new(AtomicU32::new(0));
    Arc::new(move |_: Option<&str>, _: Option<&Path>| {
        let seen = count.fetch_add(1, Ordering::SeqCst) as usize;
        if seen < calls {
            return TrustState::Trusted {
                digest: "approved".into(),
            };
        }
        later.clone()
    })
}

/// An edit in between: the worktree is no longer approved.
fn edited_after(calls: usize) -> TrustedOf {
    let edited = TrustState::Changed {
        digest: "edited".into(),
        files: vec![".mcp.json".into()],
    };
    turns_after(calls, edited)
}

#[test]
fn an_edit_between_the_gate_and_the_run_aborts_the_start() {
    for calls in [1, 2] {
        let env = Env::new();
        let runner = runner_with(&env, edited_after(calls));
        let task = env.task();
        let result = runner.start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        );
        assert!(matches!(result, Err(AgentError::Untrusted(_))), "{calls}");
        assert_eq!(env.pty_count(), 0, "{calls}");
    }
    let env = Env::new();
    let runner = runner_with(&env, edited_after(3));
    let task = env.task();
    let started = runner.start(
        &task.id,
        "claude-code",
        None,
        None,
        None,
        &OwnAgent::default(),
    );
    assert!(started.is_ok());
    assert_eq!(env.pty_count(), 1);
}

#[cfg(unix)]
#[test]
fn a_failed_check_stops_the_script_before_the_agent() {
    use std::os::unix::fs::PermissionsExt;

    let env = Env::new();
    let task = env.task();
    {
        let _strict = strict();
        env.start(&task, "claude-code", "Go");
    }
    let bin = env.dir.0.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let mark = env.dir.0.join("agent-ran");
    let program = |name: &str, body: &str| {
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    program("claude", "touch \"$MARK\"");
    let run = |code: u8| {
        program("natamux", &format!("exit {code}"));
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let script = env.script(&task.id);
        std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(". '{}'", script.display()))
            .env("PATH", path)
            .env("MARK", &mark)
            .status()
            .unwrap();
    };

    run(1);
    assert!(!mark.exists(), "the agent ran after a failed check");
    run(0);
    assert!(mark.exists(), "the agent did not run after a passed check");
}

// T-0375 R1-1: Personal mode takes back an old trust too, at the gate and when a recheck aborts.
#[test]
fn personal_mode_takes_back_the_old_trust_of_a_changed_worktree() {
    for trusted_of in [trust_always(false), edited_after(1)] {
        let env = Env::new();
        let (paths, marker) = old_markers(&env);
        let runner = runner_with(&env, trusted_of);
        let task = env.task();
        let own = OwnAgent::default();
        let _ = runner.start(&task.id, "claude-code", None, None, None, &own);
        assert!(!marker.exists(), "the old Cursor marker is gone");
        let copilot = std::fs::read_to_string(&paths.copilot_config).unwrap();
        assert!(!copilot.contains(&env.workspace), "{copilot}");
    }
}

// T-0375 R1-5: a new digest that is approved too (Natamux's own hook rewrite) is the one the
// start goes on with.
#[test]
fn an_approved_new_digest_starts_with_that_digest() {
    let env = Env::new();
    let rewritten = TrustState::Trusted {
        digest: "rewritten".into(),
    };
    let runner = runner_with(&env, turns_after(1, rewritten));
    let task = env.task();
    let _strict = strict();
    let own = OwnAgent::default();
    runner
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();
    let script = env.script_text(&task.id);
    let verify = "natamux repo verify-launch --task 'T-0001' --digest 'rewritten' && ";
    assert!(script.starts_with(verify), "{script}");
}

// The T-0373 spec test and T-0375 R1-5 end to end: the user approves the exact digest of a
// worktree that holds an old Natamux hook file, and the Strict start works the first time.
#[test]
fn approving_the_exact_digest_of_a_worktree_lets_the_strict_start_run() {
    let _strict = strict();
    let (root, repo) = test_repo();
    std::fs::write(repo.join(".mcp.json"), "{}\n").unwrap();
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-q", "-m", "config"]);
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let list = json!([{ "id": "default", "name": "app", "path": repo.to_string_lossy() }]);
    std::fs::write(data.join("repos.json"), list.to_string()).unwrap();
    let repos = Repos::new(&data, Arc::new(|| 0_i64));
    let project = repos.trust_view("default").unwrap().hash;
    repos.approve("default", &project).unwrap();

    let worktree = root.path().join("task");
    let target = worktree.to_string_lossy().to_string();
    run_git(
        &repo,
        &["worktree", "add", "-q", "-b", "task", &target, "main"],
    );
    // A hook file an older Natamux wrote: the start writes it again.
    std::fs::create_dir_all(worktree.join(".codex")).unwrap();
    let old = "# Natamux hooks: Natamux writes this file again.\n[old]\n";
    std::fs::write(worktree.join(".codex/config.toml"), old).unwrap();
    exclude_from_git(&worktree, "/.codex/config.toml");
    let view = repos.trust_view_in("default", Some(&worktree)).unwrap();
    assert!(!view.trusted);
    repos
        .approve_in("default", Some(&worktree), &view.hash)
        .unwrap();

    let mut env = Env::new();
    env.workspace = target;
    let seen = Arc::clone(&repos);
    let trusted_of: TrustedOf = Arc::new(move |id: Option<&str>, folder: Option<&Path>| {
        seen.is_trusted_in_task(id, folder)
    });
    let runner = runner_with(&env, trusted_of);
    let task = env.task();
    let own = OwnAgent::default();
    runner
        .start(&task.id, "codex", Some("Go"), None, None, &own)
        .unwrap();
    assert_eq!(env.pty_count(), 1);
    let script = env.script_text(&task.id);
    assert!(script.starts_with("natamux repo verify-launch"), "{script}");
    let digest = format!("--digest '{project}' && ");
    assert!(script.contains(&digest), "{script}");
}
