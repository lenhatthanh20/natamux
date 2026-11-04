use super::*;
use crate::app::theme_catalog::theme_slug;

fn parse(args: &[&str]) -> Result<Options, String> {
    Options::parse(args.iter().map(|arg| arg.to_string()))
}

#[test]
fn no_flags_follow_the_system() {
    let options = parse(&[]).unwrap();
    assert!(!options.smoke);
    assert_eq!(options.theme, None);
    assert_eq!(options.theme_name, None);
    assert_eq!(options.size, None);
    assert_eq!(options.hold_seconds, None);
    assert!(!options.fake_daemon);
    assert_eq!(options.gate2, None);
    assert_eq!(options.gate2_terminals, 10);
    assert_eq!(options.gate2_output, Output::Flood);
    assert_eq!(options.shot, None);
    assert!(!options.count_draws);
    assert_eq!(options.open_task, None);
    assert!(!options.bench_flows);
    assert_eq!(options.bench_tasks, 200);
}

#[test]
fn bench_flows_run_on_the_fake_daemon() {
    let options = parse(&["--bench-flows", "--bench-tasks", "50"]).unwrap();
    assert!(options.bench_flows);
    assert!(options.fake_daemon);
    assert_eq!(options.bench_tasks, 50);
    assert!(parse(&["--bench-tasks", "0"]).is_err());
    assert!(parse(&["--bench-tasks", "many"]).is_err());
    assert!(parse(&["--bench-tasks"]).is_err());
}

#[test]
fn reads_every_flag() {
    let options = parse(&[
        "--fake-daemon",
        "--smoke",
        "--theme",
        "light",
        "--size",
        "1600x1000",
        "--hold",
        "12",
        "--gate2",
        "T-0145",
        "--shot",
        "task-context-menu",
        "--count-draws",
        "--open-task",
        "T-0146",
    ])
    .unwrap();
    assert!(options.count_draws);
    assert_eq!(options.open_task.as_deref(), Some("T-0146"));
    assert_eq!(options.shot, Some(Shot::TaskContextMenu));
    assert_eq!(options.gate2.as_deref(), Some("T-0145"));
    assert!(options.smoke);
    assert_eq!(options.theme, Some(ThemeMode::Light));
    assert_eq!(options.size, Some((1600, 1000)));
    assert_eq!(options.hold_seconds, Some(12));
    assert!(options.fake_daemon);
}

#[test]
fn gate_2_takes_a_terminal_count_and_an_output() {
    let args = ["--gate2", "T-1", "--terminals", "30", "--output", "agent"];
    let options = parse(&args).unwrap();
    assert_eq!(options.gate2_terminals, 30);
    assert_eq!(options.gate2_output, Output::Agent);
    assert_eq!(parse(&["--terminals", "60"]).unwrap().gate2_terminals, 60);
    for bad in ["0", "61", "many", "-1", ""] {
        assert!(parse(&["--terminals", bad]).is_err(), "{bad}");
    }
    assert!(parse(&["--output", "loud"]).is_err());
    assert!(parse(&["--output"]).is_err());
}

#[test]
fn refuses_bad_values() {
    assert!(parse(&["--theme", "blue"]).is_err());
    assert!(parse(&["--size", "1280"]).is_err());
    assert!(parse(&["--size", "10x10"]).is_err());
    assert!(parse(&["--hold", "soon"]).is_err());
    assert!(parse(&["--hold"]).is_err());
    assert!(parse(&["--gate2"]).is_err());
    assert!(parse(&["--fast"]).is_err());
    assert!(parse(&["--shot", "tasks"]).is_err());
    assert!(parse(&["--shot"]).is_err());
    assert_eq!(
        parse(&["--open-task"]),
        Err("--open-task needs a value".to_string())
    );
}

#[test]
fn opens_a_task_with_the_fake_daemon_or_with_gate_2() {
    let options = parse(&["--fake-daemon", "--open-task", "T-0001"]).unwrap();
    assert!(options.fake_daemon);
    assert_eq!(options.open_task.as_deref(), Some("T-0001"));
    let options = parse(&["--open-task", "T-0001", "--gate2", "T-0002"]).unwrap();
    assert_eq!(options.open_task.as_deref(), Some("T-0001"));
    assert_eq!(options.gate2.as_deref(), Some("T-0002"));
}

#[test]
fn every_shot_name_parses_back() {
    for shot in Shot::ALL {
        assert_eq!(parse(&["--shot", shot.name()]).unwrap().shot, Some(shot));
    }
}

/// CI shoots the states its workflow lists: the same names, in the same order.
#[test]
fn the_ci_workflow_shoots_every_state() {
    let workflow = include_str!("../../../../.github/workflows/rust.yml");
    let part = |start: &str, end: &str| -> Vec<&str> {
        let text = workflow
            .split(start)
            .nth(1)
            .and_then(|rest| rest.split(end).next());
        text.unwrap().split_whitespace().collect()
    };
    let names: Vec<&str> = Shot::ALL.iter().map(|shot| shot.name()).collect();
    assert_eq!(part("STATES: >-", "DOCKED:"), names);
    // The docked size: the table, and every state of screens 2 to 5.
    let docked = part("DOCKED: >-", "run:");
    assert_eq!(docked.first(), Some(&"tasks-table"));
    assert_eq!(docked[1..], names[16..]);
}

#[test]
fn takes_a_theme_by_its_flag_name() {
    let options = parse(&["--theme-name", "rose-pine-dawn", "--theme", "dark"]).unwrap();
    assert_eq!(
        options.theme_name,
        Some(("Rosé Pine Dawn", ThemeKind::Light))
    );
    assert_eq!(options.theme, Some(ThemeMode::Dark));
    let options = parse(&["--theme-name", "tokyo-night"]).unwrap();
    assert_eq!(options.theme_name, Some(("Tokyo Night", ThemeKind::Dark)));
    assert!(parse(&["--theme-name", "Tokyo Night"]).is_err());
    assert!(parse(&["--theme-name", "solarized"]).is_err());
    assert!(parse(&["--theme-name"]).is_err());
}

/// D19 of the MIT plan: three states in each theme but the two defaults, at one size.
#[test]
fn the_ci_workflow_shoots_three_states_in_every_other_theme() {
    let workflow = include_str!("../../../../.github/workflows/rust.yml");
    let line = |key: &str| -> Vec<&str> {
        let found = workflow
            .lines()
            .find_map(|line| line.trim().strip_prefix(key));
        found.unwrap().split_whitespace().collect()
    };
    assert_eq!(
        line("THEMED: "),
        [
            "tasks-table",
            "workspace-changes-diff",
            "workspace-terminal-running"
        ]
    );
    let mut want: Vec<String> = Vec::new();
    for kind in [ThemeKind::Light, ThemeKind::Dark] {
        for name in &kind.themes()[1..] {
            want.push(theme_slug(name));
        }
    }
    assert_eq!(line("THEMES: "), want);
    for slug in &want {
        assert!(theme_of_slug(slug).is_some(), "{slug}");
    }
    for state in line("THEMED: ") {
        assert!(parse(&["--shot", state]).is_ok(), "{state}");
    }
}

#[test]
fn names_a_code_state_after_each_language() {
    let names: Vec<&str> = Shot::ALL
        .iter()
        .filter_map(|shot| match shot {
            Shot::Code(lang) => Some(lang.state()),
            _ => None,
        })
        .collect();
    assert_eq!(names.len(), 22);
    assert!(names.iter().all(|name| name.starts_with("code-")));
    assert!(names.contains(&"code-markdown-inline"));
    assert!(names.contains(&"code-typescript"));
}
