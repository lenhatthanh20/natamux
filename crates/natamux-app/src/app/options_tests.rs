use super::*;

fn parse(args: &[&str]) -> Result<Options, String> {
    Options::parse(args.iter().map(|arg| arg.to_string()))
}

#[test]
fn no_flags_follow_the_system() {
    let options = parse(&[]).unwrap();
    assert!(!options.smoke);
    assert_eq!(options.theme, None);
    assert_eq!(options.size, None);
    assert_eq!(options.hold_seconds, None);
    assert!(!options.fake_daemon);
    assert_eq!(options.gate2, None);
    assert_eq!(options.gate2_terminals, 10);
    assert_eq!(options.gate2_output, Output::Flood);
    assert_eq!(options.shot, None);
    assert!(!options.count_draws);
    assert_eq!(options.open_task, None);
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
