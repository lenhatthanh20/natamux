use super::*;

fn env_of(pairs: &[(&str, &str)]) -> Env {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

// shellProcess

#[cfg(unix)]
#[test]
fn runs_the_line_in_a_login_shell_in_the_workspace_as_its_own_group() {
    let env = env_of(&[
        ("SHELL", "/bin/sh"),
        ("PATH", "/usr/bin"),
        ("HOME", "/home/me"),
        ("NATAMUX_TOKEN", "t"),
        ("NATAMUX_PORT", "1"),
    ]);
    let shell = shell_process("claude -p", "/work/T-1", &env, false);
    assert_eq!(
        shell,
        ShellProcess {
            program: "/bin/sh".to_string(),
            args: vec![
                "-l".to_string(),
                "-c".to_string(),
                "cd '/work/T-1' && claude -p".to_string()
            ],
            env: env_of(&[
                ("SHELL", "/bin/sh"),
                ("PATH", "/usr/bin"),
                ("HOME", "/home/me")
            ]),
            detached: true,
        }
    );
}

#[test]
fn runs_powershell_on_windows() {
    let env = env_of(&[("SHELL", "/bin/sh"), ("NATAMUX_TOKEN", "t")]);
    let shell = shell_process("claude -p", "C:\\work", &env, true);
    assert_eq!(shell.program, "powershell.exe");
    assert_eq!(
        shell.args,
        [
            "-Command",
            "Set-Location -LiteralPath 'C:\\work'; claude -p"
        ]
    );
    assert!(!shell.detached);
}

#[test]
fn works_without_the_daemon_variables() {
    assert!(shell_process("x", "/w", &Env::new(), true).env.is_empty());
}

// The real run, with a script in place of the agent. Windows has no `sh` scripts.
#[cfg(unix)]
mod real_runs {
    use std::fs::{read_to_string, write};
    use std::os::unix::fs::PermissionsExt;

    use natamux_git::commit_drafter::{DraftInput, draft_commit_message};
    use natamux_git::test_support::TempDir;
    use natamux_proto::workspaces::changes::{ChangeStatus, DraftSource};
    use natamux_proto::workspaces::workspace::ChangeEntry;

    use super::*;

    fn fake_agent(dir: &TempDir, body: &str) -> String {
        let path = dir.path().join("fake-agent");
        write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn agent_run(dir: &TempDir, program: &str, timeout_ms: u64) -> AgentRun {
        AgentRun {
            agent: "claude-code".to_string(),
            program: program.to_string(),
            prompt: "Write it".to_string(),
            cwd: dir.path().to_path_buf(),
            timeout_ms,
        }
    }

    fn run(dir: &TempDir, program: &str, timeout_ms: u64) -> Result<String, String> {
        run_agent_once(agent_run(dir, program, timeout_ms))
    }

    #[test]
    fn gives_the_prompt_on_stdin_in_the_workspace_and_resolves_with_what_it_prints() {
        let dir = TempDir::new("once");
        let program = fake_agent(&dir, "echo \"$1 in $(pwd)\"; cat");
        assert_eq!(
            run(&dir, &program, 10_000).unwrap(),
            format!("-p in {}\nWrite it", dir.path().display())
        );
    }

    #[test]
    fn does_not_pass_on_the_variables_that_reach_the_daemon() {
        // Node stubs the process environment; here the daemon's environment is a parameter.
        let dir = TempDir::new("once");
        let program = fake_agent(&dir, "echo \"token=$NATAMUX_TOKEN port=$NATAMUX_PORT\"");
        let mut env: Env = std::env::vars().collect();
        env.insert("NATAMUX_TOKEN".to_string(), "secret".to_string());
        env.insert("NATAMUX_PORT".to_string(), "1234".to_string());
        let out = run_with_env(agent_run(&dir, &program, 10_000), &env).unwrap();
        assert_eq!(out.trim(), "token= port=");
    }

    #[test]
    fn rejects_with_the_exit_code_and_the_first_line_of_stderr() {
        let dir = TempDir::new("once");
        let program = fake_agent(&dir, "echo \"not logged in\" >&2; echo more >&2; exit 3");
        assert_eq!(
            run(&dir, &program, 10_000).unwrap_err(),
            format!("{program} exited with code 3: not logged in")
        );
    }

    #[test]
    fn ends_an_agent_that_takes_too_long() {
        let dir = TempDir::new("once");
        let program = fake_agent(&dir, "sleep 30");
        let started = Instant::now();
        assert_eq!(
            run(&dir, &program, 300).unwrap_err(),
            format!("{program} timed out after 0 s")
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn rejects_when_the_program_is_not_there_and_names_it() {
        let dir = TempDir::new("once");
        let missing = dir.path().join("missing").to_string_lossy().into_owned();
        let error = run(&dir, &missing, 10_000).unwrap_err();
        assert!(
            error.starts_with(&format!("{missing} exited with code 127")),
            "{error}"
        );
    }

    #[test]
    fn keeps_the_reason_short_when_stderr_is_empty() {
        let dir = TempDir::new("once");
        let program = fake_agent(&dir, "exit 2");
        assert_eq!(
            run(&dir, &program, 10_000).unwrap_err(),
            format!("{program} exited with code 2")
        );
    }

    #[test]
    fn keeps_path_and_home() {
        let dir = TempDir::new("once");
        let program = fake_agent(&dir, "echo \"$HOME\"; test -n \"$PATH\" && echo path");
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(
            run(&dir, &program, 10_000).unwrap().trim(),
            format!("{home}\npath")
        );
    }

    #[test]
    fn ends_the_processes_the_agent_started_too() {
        let dir = TempDir::new("once");
        let pid_file = dir.path().join("child.pid");
        let program = fake_agent(
            &dir,
            &format!("sleep 30 & echo $! > '{}'; wait", pid_file.display()),
        );
        // Node waits 500 ms; tests here run side by side, so the shell gets longer to start.
        assert!(
            run(&dir, &program, 3_000)
                .unwrap_err()
                .contains("timed out")
        );
        let pid = read_to_string(&pid_file).unwrap().trim().to_string();
        let alive = || {
            Command::new("kill")
                .args(["-0", &pid])
                .output()
                .is_ok_and(|out| out.status.success())
        };
        let until = Instant::now() + Duration::from_secs(5);
        while alive() && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive());
    }

    #[test]
    fn rejects_when_the_shell_cannot_start_and_the_draft_falls_back() {
        let dir = TempDir::new("once");
        let cwd = dir.path().join("gone");
        let gone = AgentRun {
            agent: "claude-code".to_string(),
            program: "x".to_string(),
            prompt: String::new(),
            cwd: cwd.clone(),
            timeout_ms: 1000,
        };
        assert!(
            run_agent_once(gone)
                .unwrap_err()
                .starts_with("x could not start")
        );
        let draft = draft_commit_message(
            &DraftInput {
                agent: "claude-code".to_string(),
                program: "claude".to_string(),
                cwd,
                title: "Fix the parser".to_string(),
                changes: vec![ChangeEntry {
                    path: "src/a.ts".to_string(),
                    status: ChangeStatus::Modified,
                    original_path: None,
                    added: None,
                    removed: None,
                }],
                diff: "src/a.ts | 2 +-\n+fixed".to_string(),
                timeout_ms: None,
            },
            &run_agent_once,
        );
        assert_eq!(draft.source, DraftSource::Draft);
        assert_eq!(draft.message, "Update src/a.ts");
        assert!(
            draft
                .reason
                .unwrap_or_default()
                .starts_with("claude could not start")
        );
    }
}
