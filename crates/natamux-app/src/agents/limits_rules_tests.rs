use super::*;

const NOW: &str = "2026-10-27T08:00:00.000Z";

fn now() -> i64 {
    parse_iso_ms(NOW).unwrap()
}

fn window(minutes: u32, used_percent: f64, resets_at: Option<&str>) -> LimitWindow {
    LimitWindow {
        minutes,
        used_percent,
        resets_at: resets_at.map(str::to_string),
    }
}

fn agent(agent: &str, windows: Vec<LimitWindow>) -> AgentLimits {
    AgentLimits {
        agent: agent.into(),
        windows,
        seen_at: NOW.into(),
    }
}

fn two() -> Vec<AgentLimits> {
    vec![
        agent(
            "codex",
            vec![window(300, 18.0, Some("2026-10-27T10:10:00.000Z"))],
        ),
        agent(
            "claude-code",
            vec![
                window(300, 84.4, Some("2026-10-27T10:30:00.000Z")),
                window(10_080, 41.0, Some("2026-10-30T09:00:00.000Z")),
            ],
        ),
    ]
}

#[test]
fn levels_by_share() {
    assert_eq!(level(79.9), Level::Normal);
    assert_eq!(level(80.0), Level::Warn);
    assert_eq!(level(100.0), Level::Full);
}

#[test]
fn says_when_a_window_resets() {
    let at = |text: &str| resets_in(Some(text), now());
    assert_eq!(at("2026-10-27T10:30:00.000Z").as_deref(), Some("in 2h 30m"));
    assert_eq!(at("2026-10-27T08:00:20.000Z").as_deref(), Some("in 1m"));
    assert_eq!(at("2026-10-30T12:00:00.000Z").as_deref(), Some("in 3d 4h"));
    assert_eq!(resets_in(None, now()), None);
}

#[test]
fn rows_put_the_most_used_agent_first() {
    let rows = limit_rows(&two(), now());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Claude Code");
    assert_eq!(rows[0].detail, "84% 5h · 41% week");
    assert_eq!(rows[0].reset, "resets in 2h 30m");
    assert_eq!(rows[0].level, Level::Warn);
    assert_eq!(rows[1].name, "Codex");
}

#[test]
fn the_chip_shows_the_agent_nearest_a_limit() {
    let chip = limits_chip(&two()).unwrap();
    assert_eq!(chip.text, "Claude 84% · 5h");
    assert_eq!(chip.short, "84%");
    assert_eq!(chip.level, Level::Warn);
    let full = vec![agent("codex", vec![window(10_080, 100.0, None)])];
    let chip = limits_chip(&full).unwrap();
    assert_eq!(
        (chip.text.as_str(), chip.level),
        ("Codex 100% · week", Level::Full)
    );
    assert_eq!(limits_chip(&[]), None);
}

#[test]
fn names_the_agents_in_use_that_told_no_limits() {
    let in_use: BTreeSet<String> = ["claude-code", "gemini", "cursor"]
        .map(str::to_string)
        .into();
    assert_eq!(without_limits(&in_use, &two()), ["Cursor", "Gemini CLI"]);
}

#[test]
fn the_picker_notes_the_share_or_full() {
    let limits = two();
    assert_eq!(
        picker_note("claude-code", &limits).as_deref(),
        Some("84% · 5h")
    );
    let full = vec![agent("codex", vec![window(300, 100.0, None)])];
    assert_eq!(picker_note("codex", &full).as_deref(), Some("full"));
    assert_eq!(picker_note("gemini", &limits), None);
}

#[test]
fn warns_once_per_window_and_groups_agents() {
    let mut warned = BTreeSet::new();
    assert_eq!(
        new_warning(&mut warned, &two()).as_deref(),
        Some("Claude Code used 84% of its plan limit.")
    );
    assert_eq!(new_warning(&mut warned, &two()), None);
    let more = vec![
        agent(
            "claude-code",
            vec![window(300, 90.0, Some("2026-10-27T10:30:00.000Z"))],
        ),
        agent("codex", vec![window(300, 81.0, None)]),
        agent("gemini", vec![window(1440, 86.0, None)]),
    ];
    assert_eq!(
        new_warning(&mut warned, &more).as_deref(),
        Some("2 agents are above 80% of their plan limit: Codex 81%, Gemini CLI 86%.")
    );
    // A new window after a reset warns again.
    let next = vec![agent(
        "codex",
        vec![window(300, 100.0, Some("2026-10-27T13:00:00.000Z"))],
    )];
    assert_eq!(
        new_warning(&mut warned, &next).as_deref(),
        Some("Codex reached its plan limit.")
    );
}

#[test]
fn limits_seen_at_start_are_not_news() {
    let mut warned = warned_already(&two());
    assert_eq!(new_warning(&mut warned, &two()), None);
}

#[test]
fn the_agents_in_use_are_those_of_tasks_of_the_last_week() {
    let task = |agent: Option<&str>, updated_at: &str| Task {
        agent: agent.map(str::to_string),
        updated_at: updated_at.into(),
        ..Task::default()
    };
    let tasks = [
        task(Some("codex"), "2026-10-26T08:00:00.000Z"),
        task(Some("gemini"), "2026-10-01T08:00:00.000Z"),
        task(None, "2026-10-27T07:00:00.000Z"),
    ];
    let in_use: Vec<String> = agents_in_use(&tasks, now()).into_iter().collect();
    assert_eq!(in_use, ["codex"]);
}

#[test]
fn a_window_that_has_reset_is_not_shown() {
    let limits = vec![agent(
        "codex",
        vec![
            window(300, 100.0, Some("2026-10-27T07:59:00.000Z")),
            window(10_080, 30.0, Some("2026-10-30T09:00:00.000Z")),
        ],
    )];
    let shown = current(&limits, now());
    assert_eq!(shown[0].windows.len(), 1);
    assert_eq!(limits_chip(&shown).unwrap().text, "Codex 30% · week");
    let gone = vec![agent(
        "codex",
        vec![window(300, 100.0, Some("2026-10-27T07:00:00.000Z"))],
    )];
    assert!(current(&gone, now()).is_empty());
}

#[test]
fn a_share_over_the_limit_shows_as_100() {
    let over = vec![agent("codex", vec![window(300, 104.0, None)])];
    assert_eq!(limits_chip(&over).unwrap().text, "Codex 100% · 5h");
}
