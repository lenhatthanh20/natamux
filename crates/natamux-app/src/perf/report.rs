//! Tools > Save Perf Report: every sample since the overlay came on, as JSON, for the gate 2
//! write-up in docs/rust-phase1.md.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::stats::PerfStats;

/// Writes the report and returns its path.
pub fn save(stats: &PerfStats) -> std::io::Result<PathBuf> {
    let folder = folder().ok_or_else(|| std::io::Error::other("no home folder"))?;
    std::fs::create_dir_all(&folder)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    let text = serde_json::to_string_pretty(&report(stats, Instant::now()))?;
    write_new(&folder, &format!("perf-{millis}"), &text)
}

/// Writes `<stem>.json`, or `<stem>-2.json` and so on when that name is taken: a report never
/// replaces another one.
fn write_new(folder: &Path, stem: &str, text: &str) -> std::io::Result<PathBuf> {
    for try_number in 1..100 {
        let name = if try_number == 1 {
            format!("{stem}.json")
        } else {
            format!("{stem}-{try_number}.json")
        };
        let path = folder.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other(format!(
        "too many reports named {stem}"
    )))
}

fn report(stats: &PerfStats, now: Instant) -> Value {
    let to_ms = |duration: &std::time::Duration| duration.as_secs_f64() * 1000.0;
    let summary = stats.summary(now, true);
    let (span, totals) = stats.output_totals(now);
    json!({
        "app": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpus": std::thread::available_parallelism().map(usize::from).unwrap_or(0),
        "summary": {
            "frames": summary.frames,
            "p50_ms": to_ms(&summary.p50),
            "p99_ms": to_ms(&summary.p99),
            "p999_ms": to_ms(&summary.p999),
            "frames_over_1_refresh": summary.over_one_refresh,
            "frames_over_2_refreshes": summary.over_two_refreshes,
            "missed_refreshes": summary.missed_refreshes,
            "over_16_7_ms": summary.over_16ms,
            "key_to_glyph_p95_ms": summary.key_to_glyph_p95.as_ref().map(to_ms),
            "key_to_echo_p95_ms": summary.key_to_echo_p95.as_ref().map(to_ms),
        },
        "output_bytes_per_second": stats
            .output_rates(now)
            .into_iter()
            .map(|(terminal, rate)| json!({ "terminal": terminal, "rate": rate }))
            .collect::<Vec<_>>(),
        "seconds": span.as_secs_f64(),
        "output_bytes": totals
            .into_iter()
            .map(|(terminal, bytes)| json!({ "terminal": terminal, "bytes": bytes }))
            .collect::<Vec<_>>(),
        "frame_ms": stats.frame_samples().map(|(took, _)| to_ms(&took)).collect::<Vec<_>>(),
        "frame_refreshes": stats.frame_samples().map(|(_, count)| count).collect::<Vec<_>>(),
        "key_to_glyph_ms": stats.key_samples().map(to_ms).collect::<Vec<_>>(),
        "key_to_echo_ms": stats.echo_samples().map(to_ms).collect::<Vec<_>>(),
    })
}

/// The OS's folder for app logs.
fn folder() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        Some(home()?.join("Library/Logs/Natamux Native"))
    } else if cfg!(windows) {
        Some(
            PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
                .join("Natamux Native")
                .join("Logs"),
        )
    } else {
        Some(home()?.join(".local/state/natamux-native"))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_report_has_the_summary_and_every_sample() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        for frame in 0..4 {
            stats.frame(start + Duration::from_millis(8 * frame));
        }
        let report = report(&stats, start + Duration::from_millis(30));
        assert_eq!(report["summary"]["frames"], 3);
        assert_eq!(report["frame_ms"].as_array().map(Vec::len), Some(3));
        assert_eq!(report["frame_refreshes"], json!([1, 1, 1]));
        assert_eq!(report["summary"]["missed_refreshes"], 0);
        assert_eq!(report["summary"]["key_to_glyph_p95_ms"], Value::Null);
        assert_eq!(report["summary"]["key_to_echo_p95_ms"], Value::Null);
        assert_eq!(report["key_to_echo_ms"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["os"], std::env::consts::OS);
    }

    #[test]
    fn two_saves_with_the_same_name_keep_both_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let folder = std::env::temp_dir().join(format!("natamux-report-test-{unique}"));
        std::fs::create_dir_all(&folder).unwrap();
        let first = write_new(&folder, "perf-1", "one").unwrap();
        let second = write_new(&folder, "perf-1", "two").unwrap();
        assert_eq!(first.file_name().unwrap(), "perf-1.json");
        assert_eq!(second.file_name().unwrap(), "perf-1-2.json");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "two");
    }
}
