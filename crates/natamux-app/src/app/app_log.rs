//! The app's own log, `logs/app.log` in the data folder beside `daemon.log`: one line per
//! event that is hard to see later, such as the link to the daemon going down.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui::App;
use natamux_client::{DashboardEvent, LinkStatus};

use super::source::Source;
use super::view_state;

const FILE: &str = "logs/app.log";
const KEEP: usize = 1024 * 1024;

/// Adds `<UTC time> <line>`. A log that cannot be written must never stop the app.
pub fn write(line: &str) {
    // Tests drive reconnects too; they must not write to the real data folder.
    if cfg!(test) {
        return;
    }
    if let Some(folder) = view_state::folder() {
        write_in(&folder, SystemTime::now(), line);
    }
}

/// Keeps the last 1 MB, from a line start. Called once when the app starts.
pub fn trim_at_start() {
    if let Some(folder) = view_state::folder() {
        trim_in(&folder, KEEP);
    }
}

fn write_in(folder: &Path, now: SystemTime, line: &str) {
    let path = folder.join(FILE);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{} {line}", rfc3339(now));
    }
}

fn trim_in(folder: &Path, keep: usize) {
    let path = folder.join(FILE);
    let Ok(bytes) = fs::read(&path) else {
        return;
    };
    let Some(start) = bytes.len().checked_sub(keep).filter(|start| *start > 0) else {
        return;
    };
    let at_line_start = bytes.get(start - 1) == Some(&b'\n');
    let from = if at_line_start {
        start
    } else {
        let rest = bytes.get(start..).unwrap_or_default();
        match rest.iter().position(|byte| *byte == b'\n') {
            Some(index) => start + index + 1,
            None => bytes.len(),
        }
    };
    let _ = fs::write(&path, bytes.get(from..).unwrap_or_default());
}

/// One `app.log` line per change of the link, so a "Reconnecting" can be read later. The fake
/// source writes nothing.
pub fn log_status(
    event: &DashboardEvent,
    down_at: &mut Option<Instant>,
    source: &Option<Arc<Source>>,
    cx: &mut App,
) {
    let DashboardEvent::Status(status) = event else {
        return;
    };
    let went_down = down_at.is_none() && matches!(status, LinkStatus::Reconnecting(_));
    let line = link_line(status, down_at, Instant::now());
    if went_down {
        crate::perf::overlay::link_lost(cx);
    }
    let fake = matches!(source.as_deref(), Some(Source::FakeDaemon) | None);
    if let (Some(line), false) = (line, fake) {
        write(&line);
    }
}

/// The `app.log` line for a link status, and the update of when the link went down.
pub fn link_line(
    status: &LinkStatus,
    down_at: &mut Option<Instant>,
    now: Instant,
) -> Option<String> {
    match status {
        // Only the first of a run of failed tries is a change.
        LinkStatus::Reconnecting(_) if down_at.is_some() => None,
        LinkStatus::Reconnecting(reason) => {
            *down_at = Some(now);
            Some(format!("link: reconnecting: {reason}"))
        }
        LinkStatus::Live => down_at.take().map(|at| {
            let secs = now.duration_since(at).as_secs();
            format!("link: live again after {secs} s")
        }),
        LinkStatus::Refused => {
            *down_at = None;
            Some("link: refused".to_string())
        }
        LinkStatus::Connecting => None,
    }
}

/// `2026-10-28T09:30:00Z`. By hand: the app has no time crate.
fn rfc3339(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let (hour, minute, second) = (rest / 3600, rest % 3600 / 60, rest % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("natamux-app-log-{name}-{unique}"))
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn the_time_is_rfc_3339_in_utc() {
        assert_eq!(rfc3339(at(0)), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(at(1_700_000_000)), "2023-11-14T22:13:20Z");
        assert_eq!(rfc3339(at(1_709_208_000)), "2024-02-29T12:00:00Z");
    }

    #[test]
    fn write_adds_a_line_each_time_and_makes_a_missing_folder() {
        let folder = temp_folder("write");
        assert!(!folder.exists());
        write_in(&folder, at(1_700_000_000), "link: refused");
        write_in(&folder, at(1_700_000_005), "link: live again after 5 s");
        let text = fs::read_to_string(folder.join(FILE)).unwrap_or_default();
        assert_eq!(
            text,
            "2023-11-14T22:13:20Z link: refused\n2023-11-14T22:13:25Z link: live again after 5 s\n"
        );
    }

    #[test]
    fn trim_keeps_the_last_bytes_from_a_line_start() {
        let folder = temp_folder("trim");
        let path = folder.join(FILE);
        assert!(fs::create_dir_all(folder.join("logs")).is_ok());
        assert!(fs::write(&path, "aaaa\nbbbb\ncccc\n").is_ok());
        // The cut falls inside "bbbb": that line goes whole.
        trim_in(&folder, 8);
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), "cccc\n");
        // The cut falls on a line start: the line stays.
        assert!(fs::write(&path, "aaaa\nbbbb\ncccc\n").is_ok());
        trim_in(&folder, 10);
        assert_eq!(
            fs::read_to_string(&path).unwrap_or_default(),
            "bbbb\ncccc\n"
        );
        // A small file stays as it is.
        trim_in(&folder, 100);
        assert_eq!(
            fs::read_to_string(&path).unwrap_or_default(),
            "bbbb\ncccc\n"
        );
    }

    #[test]
    fn trim_of_a_missing_file_does_nothing() {
        trim_in(&temp_folder("none"), 10);
    }
}

#[cfg(test)]
mod link_tests {
    use std::time::Duration;

    use natamux_client::ClientError;

    use super::*;

    fn lost(reason: &str) -> LinkStatus {
        LinkStatus::Reconnecting(ClientError::Unreachable {
            url: "http://d".into(),
            reason: reason.into(),
        })
    }

    #[test]
    fn reconnecting_logs_its_reason_once_per_run_of_failed_tries() {
        let (mut down, now) = (None, Instant::now());
        let line = link_line(&lost("refused"), &mut down, now);
        assert_eq!(
            line.as_deref(),
            Some("link: reconnecting: Cannot reach the daemon at http://d: refused")
        );
        assert_eq!(down, Some(now));
        assert_eq!(link_line(&lost("again"), &mut down, now), None);
        assert_eq!(down, Some(now));
    }

    #[test]
    fn live_again_says_how_long_the_link_was_down() {
        let now = Instant::now();
        let mut down = Some(now);
        let later = now + Duration::from_secs(7);
        let line = link_line(&LinkStatus::Live, &mut down, later);
        assert_eq!(line.as_deref(), Some("link: live again after 7 s"));
        assert_eq!(down, None);
    }

    #[test]
    fn live_with_no_loss_and_connecting_log_nothing() {
        let mut down = None;
        assert_eq!(
            link_line(&LinkStatus::Live, &mut down, Instant::now()),
            None
        );
        assert_eq!(
            link_line(&LinkStatus::Connecting, &mut down, Instant::now()),
            None
        );
        assert_eq!(down, None);
    }

    #[test]
    fn refused_logs_and_forgets_the_loss() {
        let now = Instant::now();
        let mut down = Some(now);
        let line = link_line(&LinkStatus::Refused, &mut down, now);
        assert_eq!(line.as_deref(), Some("link: refused"));
        assert_eq!(down, None);
    }
}
