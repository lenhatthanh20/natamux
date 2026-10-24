//! Ports `packages/daemon/src/stages/__tests__/stage-checks.test.ts`.

use std::path::Path;
use std::sync::Mutex;
#[cfg(unix)]
use std::time::Duration;

use natamux_proto::stages::schema::{Gate, Stage, StageKind};

use super::*;
use natamux_git::test_support::{TempDir, test_repo};

fn input(folder: &Path, stage: Stage, started_at: Option<&str>) -> CheckInput {
    CheckInput {
        stage,
        folder: folder.to_path_buf(),
        cwd: folder.to_path_buf(),
        is_worktree: false,
        started_at: started_at.map(str::to_string),
        start_head: None,
    }
}

fn no_command() -> RunCommand {
    Arc::new(|_, _| Ran {
        code: 0,
        output: String::new(),
    })
}

fn run(folder: &Path, stage: Stage, started_at: Option<&str>) -> CheckResult {
    run_stage_checks(&input(folder, stage, started_at), &no_command()).unwrap()
}

fn agent(id: &str, output: Option<&str>) -> Stage {
    Stage {
        output: output.map(str::to_string),
        ..Stage::new(id, StageKind::Agent, Gate::Manual)
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn set_time(path: &Path, millis: i64) {
    let time = natamux_core::js::time_from_millis(millis);
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}

#[test]
fn passes_a_written_output_with_its_sections() {
    let dir = TempDir::new("checks");
    std::fs::write(
        dir.path().join("plan.md"),
        "# Steps\n1. a\n## Files\n- a.ts\n## Tests\n- one\n",
    )
    .unwrap();
    let stage = Stage {
        sections: Some(strings(&["Steps", "Files", "Tests"])),
        ..agent("plan", Some("plan.md"))
    };
    assert_eq!(
        run(dir.path(), stage, None),
        CheckResult {
            passed: true,
            reasons: vec![],
            passes: strings(&[
                "plan.md was written after the stage started",
                "Sections Steps, Files, Tests are there",
            ]),
        }
    );
}

#[test]
fn fails_a_missing_empty_or_old_output() {
    let dir = TempDir::new("checks");
    let stage = agent("spec", Some("spec.md"));
    assert_eq!(
        run(dir.path(), stage.clone(), None).reasons,
        strings(&[".natamux-task/spec.md is missing"])
    );

    std::fs::write(dir.path().join("spec.md"), "  \n").unwrap();
    assert_eq!(
        run(dir.path(), stage.clone(), None).reasons,
        strings(&["spec.md is empty"])
    );

    let path = dir.path().join("spec.md");
    std::fs::write(&path, "# Goal\n").unwrap();
    // 2026-01-01T00:00:00.000Z
    set_time(&path, 1_767_225_600_000);
    assert_eq!(
        run(dir.path(), stage, Some("2026-06-01T00:00:00.000Z")).reasons,
        strings(&["spec.md was not changed after the stage started"])
    );
}

#[test]
fn passes_an_output_whose_time_reads_a_little_before_the_stage_started() {
    let dir = TempDir::new("checks");
    let stage = Stage::new("context", StageKind::Context, Gate::Manual);
    let stage = Stage {
        output: Some("context.md".into()),
        ..stage
    };
    let started_at = "2026-06-01T00:00:00.000Z";
    let started = natamux_core::js::parse_iso_millis(started_at).unwrap();
    let path = dir.path().join("context.md");
    std::fs::write(&path, "# Export\n").unwrap();

    // The file clock of Linux is coarse: a file written right after the start can read 10 ms early.
    set_time(&path, started - 10);
    assert!(run(dir.path(), stage.clone(), Some(started_at)).passed);

    set_time(&path, started - 2000);
    assert_eq!(
        run(dir.path(), stage, Some(started_at)).reasons,
        strings(&["context.md was not changed after the stage started"])
    );
}

#[test]
fn counts_the_cases_of_a_test_spec() {
    let dir = TempDir::new("checks");
    let stage = Stage {
        sections: Some(strings(&["Cases"])),
        ..agent("test-spec", Some("test-spec.md"))
    };
    let path = dir.path().join("test-spec.md");
    std::fs::write(&path, "# Cases\n\nNone yet.\n").unwrap();
    assert_eq!(
        run(dir.path(), stage.clone(), None).reasons,
        strings(&["test-spec.md has no case under Cases"])
    );

    std::fs::write(&path, "# Cases\n### Empty list\nx\n### Commas\ny\n").unwrap();
    assert!(
        run(dir.path(), stage, None)
            .passes
            .contains(&"Section Cases has 2 cases".to_string())
    );
}

#[test]
fn needs_a_workspace_for_the_commit_checks() {
    let dir = TempDir::new("checks");
    let stage = Stage {
        require_commit: Some(true),
        ..Stage::new("implement", StageKind::Agent, Gate::Auto)
    };
    assert_eq!(
        run(dir.path(), stage, None).reasons,
        strings(&["The task has no workspace"])
    );
}

#[test]
fn runs_the_check_command_and_keeps_the_end_of_its_output() {
    let dir = TempDir::new("checks");
    let stage = Stage {
        check: Some("pnpm typecheck".into()),
        ..Stage::new("implement", StageKind::Agent, Gate::Auto)
    };
    let failing: RunCommand = Arc::new(|_, _| Ran {
        code: 2,
        output: "error TS1".into(),
    });
    let result = run_stage_checks(&input(dir.path(), stage.clone(), None), &failing).unwrap();
    assert_eq!(
        result.reasons,
        strings(&["pnpm typecheck failed with exit code 2:\nerror TS1"])
    );

    let result = run(dir.path(), stage, None);
    assert!(result.passed);
    assert_eq!(result.passes, strings(&["pnpm typecheck passed"]));
}

#[test]
fn runs_its_commands_at_the_root_of_the_worktree_with_no_stage_folder_there() {
    let (_root, repo) = test_repo();
    let stage = Stage {
        require_commit: Some(true),
        require_clean: Some(true),
        check: Some("./test.sh".into()),
        ..Stage::new("implement", StageKind::Agent, Gate::Auto)
    };
    let ran = std::sync::Arc::new(Mutex::new(Vec::new()));
    let seen = ran.clone();
    let command: RunCommand = Arc::new(move |_, cwd| {
        seen.lock().unwrap().push(cwd.to_path_buf());
        Ran {
            code: 0,
            output: String::new(),
        }
    });
    let result = run_stage_checks(
        &CheckInput {
            stage,
            folder: repo.join(".natamux-task"),
            cwd: repo.clone(),
            is_worktree: true,
            started_at: None,
            start_head: None,
        },
        &command,
    )
    .unwrap();
    assert_eq!(result.reasons, Vec::<String>::new());
    assert_eq!(*ran.lock().unwrap(), vec![repo]);
}

#[test]
fn uses_sh_on_posix_and_cmd_exe_on_windows() {
    assert_eq!(
        shell_command("make test", false),
        ("sh".to_string(), strings(&["-c", "make test"]))
    );
    assert_eq!(
        shell_command("make test", true),
        (
            "cmd.exe".to_string(),
            strings(&["/d", "/s", "/c", "\"make test\""])
        )
    );
}

#[test]
fn counts_the_list_items_under_a_section_only_not_their_own_lists() {
    let text = "# Cases\n- a\n  - input: x\n1. b\n# Notes\n- not a case\n";
    assert_eq!(count_items(text, "Cases"), 2);
    assert_eq!(count_items(text, "Missing"), 0);
}

#[test]
fn counts_sub_headings_when_the_cases_have_them() {
    let text = "# Cases\n### Empty\n- input: \"\"\n- expected: \"\"\n### Comma\n- input: a,b\n";
    assert_eq!(count_items(text, "Cases"), 2);
}

// Cases of my own: the shell runner and the heading scanner.

#[cfg(unix)]
#[test]
fn runs_a_shell_command_with_its_code_and_output() {
    let dir = TempDir::new("shell");
    let ok = run_shell_command("echo out; echo err >&2", dir.path());
    assert_eq!(
        ok,
        Ran {
            code: 0,
            output: "out\nerr\n".into()
        }
    );
    let failed = run_shell_command("echo no; exit 3", dir.path());
    assert_eq!(
        failed,
        Ran {
            code: 3,
            output: "no\n".into()
        }
    );
    let cwd = run_shell_command("pwd", dir.path());
    assert_eq!(cwd.output.trim(), dir.path().to_string_lossy());
}

#[cfg(unix)]
#[test]
fn kills_a_command_that_runs_too_long() {
    let dir = TempDir::new("shell");
    let started = std::time::Instant::now();
    let ran = run_shell_command_with_timeout(
        "echo start; sleep 30",
        dir.path(),
        Duration::from_millis(300),
    );
    assert_eq!(ran.code, 1);
    assert_eq!(ran.output, "start\n\n(timed out)");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn stops_a_command_with_more_than_10_mb_of_output() {
    let dir = TempDir::new("shell");
    let ran = run_shell_command("yes | head -c 20000000", dir.path());
    assert_eq!(ran.code, 1);
    assert!(ran.output.len() < 11 * 1024 * 1024);
}

#[test]
fn reads_headings_as_the_regex_does() {
    assert_eq!(
        headings("# Plan #\n## Size ##  \n####### no\n#nospace\n### C#\r\nplain\n"),
        strings(&["plan", "size", "c"])
    );
    assert_eq!(
        section_body("# A\nx\n## B\ny\n# C\nz", "a"),
        strings(&["x", "## B", "y"])
    );
}
