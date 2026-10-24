//! The price table of `model-prices.ts`: USD per million tokens. `cache_write` is the short cache
//! (Claude 5 minutes, OpenAI 30 minutes), `cache_write_1h` the long one (Claude 1 hour, OpenAI 12
//! hours). API list prices, standard tier, short prompts, read 2026-10-21.

use super::UsageCounts;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub cache_write_1h: f64,
    pub output: f64,
}

const fn price(
    input: f64,
    cache_read: f64,
    cache_write: f64,
    cache_write_1h: f64,
    output: f64,
) -> Price {
    Price {
        input,
        cache_read,
        cache_write,
        cache_write_1h,
        output,
    }
}

// A model without a cache write price bills cache writes as input.
const fn no_write(input: f64, cache_read: f64, output: f64) -> Price {
    price(input, cache_read, input, input, output)
}

// Keys are model ids in the form of `normalize_model`.
const PRICES: [(&str, Price); 41] = [
    ("claude-fable-5-1", price(10.0, 0.25, 12.5, 20.0, 50.0)),
    ("claude-mythos-5-1", price(10.0, 0.25, 12.5, 20.0, 50.0)),
    ("claude-fable-5", price(10.0, 1.0, 12.5, 20.0, 50.0)),
    ("claude-mythos-5", price(10.0, 1.0, 12.5, 20.0, 50.0)),
    ("claude-opus-5-5", price(4.0, 0.2, 5.0, 8.0, 20.0)),
    ("claude-opus-5", price(5.0, 0.5, 6.25, 10.0, 25.0)),
    ("claude-opus-4-8", price(5.0, 0.5, 6.25, 10.0, 25.0)),
    ("claude-opus-4-7", price(5.0, 0.5, 6.25, 10.0, 25.0)),
    ("claude-opus-4-6", price(5.0, 0.5, 6.25, 10.0, 25.0)),
    ("claude-sonnet-5-5", price(2.0, 0.2, 2.5, 4.0, 10.0)),
    ("claude-sonnet-5", price(2.0, 0.2, 2.5, 4.0, 10.0)),
    ("claude-sonnet-4-6", price(3.0, 0.3, 3.75, 6.0, 15.0)),
    // Prompts of 100K tokens or fewer; longer ones cost 5 times more.
    ("claude-haiku-5-5", price(0.1, 0.01, 0.125, 0.2, 0.5)),
    ("claude-haiku-4-5", price(1.0, 0.1, 1.25, 2.0, 5.0)),
    ("gpt-6-astra", price(10.0, 1.0, 12.5, 25.0, 50.0)),
    ("gpt-6-1-sol", price(2.0, 0.1, 2.5, 5.0, 10.0)),
    ("gpt-6-sol", price(2.0, 0.2, 2.5, 5.0, 10.0)),
    ("gpt-6-luna", price(0.1, 0.01, 0.125, 0.25, 0.5)),
    ("gpt-5-6-sol", price(4.0, 0.4, 5.0, 10.0, 20.0)),
    ("gpt-5-6-terra", price(2.0, 0.2, 2.5, 5.0, 12.0)),
    ("gpt-5-6-luna", price(0.2, 0.02, 0.25, 0.5, 1.2)),
    ("gpt-5-5", no_write(5.0, 0.5, 30.0)),
    ("gpt-5-4", no_write(2.5, 0.25, 15.0)),
    ("gpt-5-4-mini", no_write(0.75, 0.075, 4.5)),
    ("gpt-5-4-nano", no_write(0.2, 0.02, 1.25)),
    ("gpt-5-3-codex", no_write(1.75, 0.175, 14.0)),
    ("gpt-5-2", no_write(1.75, 0.175, 14.0)),
    ("gpt-5-1", no_write(1.25, 0.125, 10.0)),
    ("gpt-5", no_write(1.25, 0.125, 10.0)),
    ("gpt-5-mini", no_write(0.25, 0.025, 2.0)),
    ("gpt-5-nano", no_write(0.05, 0.005, 0.4)),
    ("gemini-4-argon", no_write(2.0, 0.1, 10.0)),
    ("gemini-3-8-flash", no_write(0.75, 0.075, 3.75)),
    ("gemini-3-6-flash", no_write(0.75, 0.075, 3.75)),
    ("gemini-3-5-flash-lite", no_write(0.3, 0.03, 2.5)),
    ("gemini-3-1-flash-lite", no_write(0.25, 0.025, 1.5)),
    // Prompts of 200K tokens or fewer, as the two below.
    ("gemini-3-1-pro-preview", no_write(2.0, 0.2, 12.0)),
    ("gemini-3-flash-preview", no_write(0.5, 0.05, 3.0)),
    ("gemini-2-5-pro", no_write(1.25, 0.125, 10.0)),
    ("gemini-2-5-flash", no_write(0.3, 0.03, 2.5)),
    ("gemini-2-5-flash-lite", no_write(0.1, 0.01, 0.4)),
];

fn digits(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

// The date suffix: `-YYYYMMDD` or `-YYYY-MM-DD`.
fn without_date(model: &str) -> &str {
    let bytes = model.as_bytes();
    let cut = |size: usize| &model[..model.len() - size];
    if bytes.len() >= 9 && bytes[bytes.len() - 9] == b'-' && digits(&bytes[bytes.len() - 8..]) {
        return cut(9);
    }
    if bytes.len() >= 11 {
        let tail = &bytes[bytes.len() - 11..];
        if tail[0] == b'-'
            && digits(&tail[1..5])
            && tail[5] == b'-'
            && digits(&tail[6..8])
            && tail[8] == b'-'
            && digits(&tail[9..11])
        {
            return cut(11);
        }
    }
    model
}

/// Copilot writes "claude-sonnet-5.5", others add a date such as "-20260401" or "-2026-04-01".
pub fn normalize_model(model: &str) -> String {
    let lower = natamux_core::js::js_trim(model).to_lowercase();
    without_date(&lower).replace('.', "-")
}

pub fn price_of(model: &str) -> Option<Price> {
    let id = normalize_model(model);
    PRICES
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, price)| *price)
}

/// None when the model has no price. The sum is in the order of the Node code, so the f64 is the
/// same.
pub fn cost_of(counts: &UsageCounts, model: &str) -> Option<f64> {
    let per = price_of(model)?;
    let cache_write_5m = counts.cache_write.saturating_sub(counts.cache_write_1h);
    let sum = counts.input as f64 * per.input
        + counts.cache_read as f64 * per.cache_read
        + cache_write_5m as f64 * per.cache_write
        + counts.cache_write_1h as f64 * per.cache_write_1h
        + counts.output as f64 * per.output;
    Some(sum / 1_000_000.0)
}

#[cfg(test)]
#[path = "prices_tests.rs"]
mod tests;
