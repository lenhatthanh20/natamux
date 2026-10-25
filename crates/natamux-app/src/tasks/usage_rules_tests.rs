use super::*;

fn row(stage: &str, cost_usd: Option<f64>) -> UsageRow {
    UsageRow {
        stage: stage.into(),
        agent: "claude-code".into(),
        model: "claude-opus-5-5".into(),
        input: 100,
        cache_read: 200,
        cache_write: 300,
        output: 400,
        cost_usd,
    }
}

#[test]
fn writes_each_count_as_text() {
    let table = [
        (950, "950"),
        (12_345, "12.3K"),
        (610_000, "610K"),
        (1_234_567, "1.2M"),
        (999_950, "1M"),
        (2_000_000_000, "2B"),
    ];
    for (count, text) in table {
        assert_eq!(format_tokens(count), text, "{count}");
    }
}

#[test]
fn writes_each_cost_as_text() {
    let table = [
        (Some(0.84), "$0.84"),
        (Some(1234.5), "$1,234.50"),
        (Some(0.004), "<$0.01"),
        (Some(0.0), "$0.00"),
        (None, "—"),
    ];
    for (cost, text) in table {
        assert_eq!(format_usd(cost), text, "{cost:?}");
    }
}

#[test]
fn names_the_estimate_and_the_tokens_or_the_tokens_alone_without_a_price() {
    assert_eq!(
        usage_line(2_400_000, Some(1.84)),
        "$1.84 est. · 2.4M tokens"
    );
    assert_eq!(usage_line(610_000, None), "610K tokens");
}

#[test]
fn sums_the_stages_rows_with_the_cost_of_the_priced_ones() {
    let usage = TaskUsage {
        tokens: 3000,
        cost_usd: Some(0.5),
        unpriced_models: vec!["mystery".into()],
        stages: vec![
            row("implement", Some(0.25)),
            row("implement", None),
            row("review", Some(0.25)),
        ],
    };
    assert_eq!(
        stage_usage(Some(&usage), "implement"),
        Some(StageUsage {
            tokens: 2000,
            cost_usd: Some(0.25)
        })
    );
    assert_eq!(stage_usage(Some(&usage), "spec"), None);
    assert_eq!(stage_usage(None, "implement"), None);
}

fn usage() -> TaskUsage {
    let first = UsageRow {
        input: 12_300,
        cache_read: 2_000_000,
        cache_write: 300_000,
        output: 87_700,
        cost_usd: Some(1.84),
        ..row("implement", None)
    };
    let second = UsageRow {
        agent: "codex".into(),
        model: "mystery-model".into(),
        input: 100,
        cache_read: 0,
        cache_write: 0,
        output: 50,
        ..row("review", None)
    };
    TaskUsage {
        tokens: 2_400_000,
        cost_usd: Some(1.84),
        unpriced_models: vec!["mystery-model".into()],
        stages: vec![first, second],
    }
}

#[test]
fn shows_the_total_then_a_row_per_stage_and_model() {
    let section = usage_section(Some("claude-code"), Some(&usage())).unwrap();
    assert_eq!(section.total.as_deref(), Some("$1.84 est. · 2.4M tokens"));
    let [first, second] = &section.rows[..] else {
        panic!("two rows")
    };
    assert_eq!(first.stage, "Implement");
    assert_eq!(first.input, "12.3K");
    assert_eq!(first.cache, "2.3M");
    assert_eq!(first.cost, "$1.84");
    assert_eq!(first.cost_tip, None);
    assert_eq!(second.cost, "—");
    assert_eq!(
        second.cost_tip.as_deref(),
        Some("No price for mystery-model")
    );
    assert_eq!(second.agent, "Codex");
    assert!(section.footer.contains("Estimated at API list prices"));
}

#[test]
fn says_when_there_is_no_usage_yet_and_that_cursor_reports_none() {
    let section = usage_section(Some("cursor"), None).unwrap();
    assert_eq!(section.total, None);
    assert!(section.rows.is_empty());
    assert!(section.footer.contains("Cursor does not report usage."));
    let empty = TaskUsage::default();
    assert_eq!(
        usage_section(Some("claude-code"), Some(&empty))
            .unwrap()
            .total,
        None
    );
}

#[test]
fn is_hidden_for_a_task_that_never_had_an_agent() {
    assert_eq!(usage_section(None, None), None);
    assert!(usage_section(None, Some(&usage())).is_some());
}

#[test]
fn shows_no_stage_for_a_row_with_no_stage() {
    assert_eq!(usage_cells(&row("", None)).stage, "No stage");
}
