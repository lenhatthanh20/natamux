use super::*;

fn counts(input: u64, cache_read: u64, cache_write: u64, output: u64) -> UsageCounts {
    UsageCounts {
        input,
        cache_read,
        cache_write,
        cache_write_1h: 0,
        output,
    }
}

fn close(got: Option<f64>, want: f64) {
    let got = got.unwrap();
    assert!((got - want).abs() < 1e-9, "{got} is not {want}");
}

#[test]
fn normalize_model_reads_the_forms_agents_write() {
    let cases = [
        ("claude-sonnet-5.5", "claude-sonnet-5-5"),
        ("Claude-Opus-4-6-20260205", "claude-opus-4-6"),
        ("gemini-2.5-pro-2026-01-15", "gemini-2-5-pro"),
        ("gpt-6.1-sol", "gpt-6-1-sol"),
    ];
    for (model, id) in cases {
        assert_eq!(normalize_model(model), id, "{model}");
    }
}

#[test]
fn price_of_knows_the_models_of_each_agent_in_the_forms_they_write() {
    assert_eq!(price_of("claude-opus-5-5").map(|p| p.input), Some(4.0));
    assert_eq!(price_of("claude-sonnet-5.5"), price_of("claude-sonnet-5-5"));
    assert!(price_of("claude-sonnet-5.5").is_some());
    assert_eq!(price_of("gpt-6.1-sol").map(|p| p.cache_read), Some(0.1));
    assert_eq!(price_of("gemini-3.8-flash").map(|p| p.output), Some(3.75));
}

#[test]
fn price_of_has_no_price_for_a_model_not_in_the_table() {
    assert_eq!(price_of("codex-auto-review"), None);
    assert_eq!(price_of("unknown"), None);
}

#[test]
fn cost_of_prices_each_kind_of_token_the_long_cache_writes_at_their_own_price() {
    let million = UsageCounts {
        cache_write_1h: 400_000,
        ..counts(1_000_000, 1_000_000, 1_000_000, 1_000_000)
    };
    // 4 input + 0.2 cache read + 0.6 x 5 short writes + 0.4 x 8 long writes + 20 output.
    close(cost_of(&million, "claude-opus-5-5"), 30.4);
}

#[test]
fn cost_of_knows_gpt_6_1_terra() {
    let million = UsageCounts {
        cache_write_1h: 400_000,
        ..counts(1_000_000, 1_000_000, 1_000_000, 1_000_000)
    };
    // 1 input + 0.05 cache read + 0.6 x 1.25 short writes + 0.4 x 2.5 long writes + 5 output.
    close(cost_of(&million, "gpt-6.1-terra"), 7.8);
}

#[test]
fn cost_of_bills_cache_writes_as_input_for_a_model_without_a_write_price() {
    close(cost_of(&counts(0, 0, 1_000_000, 0), "gemini-2.5-pro"), 1.25);
}

#[test]
fn cost_of_is_none_for_a_model_without_a_price() {
    assert_eq!(cost_of(&counts(10, 0, 0, 10), "codex-auto-review"), None);
}
