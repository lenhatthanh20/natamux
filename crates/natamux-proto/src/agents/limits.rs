//! The plan limits of the agents (`GET /limits`, `limits.changed`): how much of each window of a
//! subscription is used, as the agent itself last told it. Only Claude Code (its status line
//! input) and Codex (its session file) tell them.

use serde::{Deserialize, Serialize};

pub const FIVE_HOURS: u32 = 300;
pub const ONE_WEEK: u32 = 10_080;

/// One window of a plan, such as the 5 hours or the week. `resets_at` is an ISO time.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub minutes: u32,
    pub used_percent: f64,
    #[serde(default)]
    pub resets_at: Option<String>,
}

/// The windows of one agent and when it last told them.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLimits {
    pub agent: String,
    pub windows: Vec<LimitWindow>,
    pub seen_at: String,
}

/// The body of `GET /limits`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct LimitList {
    #[serde(default)]
    pub limits: Vec<AgentLimits>,
}

impl LimitWindow {
    /// "5h", "week", "day", or the hours or minutes of another window.
    pub fn short_name(&self) -> String {
        match self.minutes {
            FIVE_HOURS => "5h".into(),
            ONE_WEEK => "week".into(),
            1440 => "day".into(),
            minutes if minutes % 60 == 0 => format!("{}h", minutes / 60),
            minutes => format!("{minutes}m"),
        }
    }
}

impl AgentLimits {
    /// The window nearest its limit.
    pub fn top(&self) -> Option<&LimitWindow> {
        self.windows
            .iter()
            .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(minutes: u32, used_percent: f64) -> LimitWindow {
        LimitWindow {
            minutes,
            used_percent,
            resets_at: None,
        }
    }

    #[test]
    fn names_the_windows() {
        let names: Vec<String> = [300, 10_080, 1440, 120, 45]
            .map(|minutes| window(minutes, 0.0).short_name())
            .into();
        assert_eq!(names, ["5h", "week", "day", "2h", "45m"]);
    }

    #[test]
    fn the_top_window_is_the_most_used() {
        let limits = AgentLimits {
            agent: "codex".into(),
            windows: vec![window(300, 12.0), window(10_080, 40.0)],
            seen_at: String::new(),
        };
        assert_eq!(limits.top().map(|w| w.minutes), Some(10_080));
    }

    #[test]
    fn reads_and_writes_camel_case() {
        let text = r#"{"agent":"codex","windows":[{"minutes":300,"usedPercent":8.5,"resetsAt":"2026-10-27T10:00:00.000Z"}],"seenAt":"2026-10-27T09:00:00.000Z"}"#;
        let limits: AgentLimits = serde_json::from_str(text).unwrap();
        assert_eq!(limits.windows[0].used_percent, 8.5);
        assert_eq!(serde_json::to_string(&limits).unwrap(), text);
    }
}
