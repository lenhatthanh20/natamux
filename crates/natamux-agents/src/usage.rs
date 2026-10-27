//! Token usage of an agent session (`usage-readers.ts`): the counts, and a reader for each agent
//! that writes its usage.

use std::path::Path;

pub mod limits;
pub mod prices;
pub mod readers;
pub mod session_file;

pub use session_file::session_file;

/// `cache_write_1h` is the part of `cache_write` that has the long cache's own price.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UsageCounts {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cache_write_1h: u64,
    pub output: u64,
}

/// A session's usage so far, by model, in the order the models first appear (a JS object).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelUsage(pub Vec<(String, UsageCounts)>);

impl ModelUsage {
    pub fn get(&self, model: &str) -> Option<&UsageCounts> {
        self.0
            .iter()
            .find(|(name, _)| name == model)
            .map(|(_, counts)| counts)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn zero_counts() -> UsageCounts {
    UsageCounts::default()
}

pub fn add_counts(usage: &mut ModelUsage, model: &str, counts: &UsageCounts) {
    let index = match usage.0.iter().position(|(name, _)| name == model) {
        Some(index) => index,
        None => {
            usage.0.push((model.to_string(), zero_counts()));
            usage.0.len() - 1
        }
    };
    let total = &mut usage.0[index].1;
    total.input += counts.input;
    total.cache_read += counts.cache_read;
    total.cache_write += counts.cache_write;
    total.cache_write_1h += counts.cache_write_1h;
    total.output += counts.output;
}

/// Cursor writes no usage, so it has no reader.
pub fn usage_reader(agent: &str) -> Option<fn(&Path) -> ModelUsage> {
    match agent {
        "claude-code" => Some(readers::read_claude_usage),
        "codex" => Some(readers::read_codex_usage),
        "copilot" => Some(readers::read_copilot_usage),
        "gemini" => Some(readers::read_gemini_usage),
        _ => None,
    }
}
