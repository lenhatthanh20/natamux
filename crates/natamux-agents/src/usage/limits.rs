//! The plan limits the agents tell: Codex in the `token_count` lines of its session file, Claude
//! Code in the JSON it gives its status line command. Only numbers leave here; a field that is
//! missing or not a number drops its window.

use std::path::Path;

use natamux_core::js::iso_from_millis;
use natamux_proto::agents::limits::{FIVE_HOURS, LimitWindow, ONE_WEEK};
use serde_json::Value;

use super::readers::json_lines;

fn percent(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .map(|number| number.clamp(0.0, 1000.0))
}

/// Seconds since 1970, as both agents write the reset time.
fn reset_time(value: Option<&Value>) -> Option<String> {
    let seconds = value?
        .as_f64()
        .filter(|number| number.is_finite() && *number > 0.0)?;
    Some(iso_from_millis((seconds * 1000.0) as i64))
}

fn codex_window(value: Option<&Value>) -> Option<LimitWindow> {
    let window = value?.as_object()?;
    let minutes = window.get("window_minutes")?.as_u64()?;
    Some(LimitWindow {
        minutes: u32::try_from(minutes).ok().filter(|minutes| *minutes > 0)?,
        used_percent: percent(window.get("used_percent"))?,
        resets_at: reset_time(window.get("resets_at")),
    })
}

/// The windows of the last `token_count` line that has `rate_limits`. None when no line has them.
pub fn read_codex_limits(path: &Path) -> Option<Vec<LimitWindow>> {
    json_lines(path).iter().rev().find_map(|entry| {
        let payload = entry.get("payload")?.as_object()?;
        if entry.get("type")?.as_str()? != "event_msg"
            || payload.get("type")?.as_str()? != "token_count"
        {
            return None;
        }
        let limits = payload.get("rate_limits")?.as_object()?;
        let windows: Vec<LimitWindow> = ["primary", "secondary"]
            .iter()
            .filter_map(|name| codex_window(limits.get(*name)))
            .collect();
        (!windows.is_empty()).then_some(windows)
    })
}

/// `rate_limits` of Claude Code's status line input: `five_hour` and `seven_day`, each with
/// `used_percentage` and `resets_at`. Either may be missing.
pub fn claude_limits(rate_limits: &Value) -> Vec<LimitWindow> {
    [("five_hour", FIVE_HOURS), ("seven_day", ONE_WEEK)]
        .iter()
        .filter_map(|(name, minutes)| {
            let window = rate_limits.get(name)?;
            Some(LimitWindow {
                minutes: *minutes,
                used_percent: percent(window.get("used_percentage"))?,
                resets_at: reset_time(window.get("resets_at")),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
