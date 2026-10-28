//! Tools > Save Perf Report: every sample since the overlay came on, as JSON, for the gate 2
//! write-up in docs/rust-phase1.md, and the start times of screen 1 (docs/rust-phase3.md).

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::stats::PerfStats;

static STARTED: OnceLock<Instant> = OnceLock::new();
static FIRST_FRAME: OnceLock<Duration> = OnceLock::new();
static FIRST_DATA: OnceLock<Duration> = OnceLock::new();

/// The start of the app: the first thing `main` does.
pub fn mark_start() {
    STARTED.get_or_init(Instant::now);
}

/// The window drew its first frame. Later calls change nothing.
pub fn mark_first_frame() {
    mark(&FIRST_FRAME);
}

/// The task list came for the first time. Later calls change nothing.
pub fn mark_first_data() {
    mark(&FIRST_DATA);
}

fn mark(cell: &OnceLock<Duration>) {
    if let Some(started) = STARTED.get() {
        cell.get_or_init(|| started.elapsed());
    }
}

/// Start to first frame, and start to the list with data, in ms.
fn start_times() -> (Option<f64>, Option<f64>) {
    let to_ms = |duration: &Duration| duration.as_secs_f64() * 1000.0;
    (FIRST_FRAME.get().map(to_ms), FIRST_DATA.get().map(to_ms))
}

/// The app's CPU time (user and system) and its resident memory now.
pub struct Usage {
    pub cpu: Duration,
    pub rss_bytes: u64,
}

/// Read with `ps`: the app has no libc or Mach crate, and a `--gate2` run reads it only a few
/// times. None when `ps` is missing or its text is not understood.
pub fn process_usage() -> Option<Usage> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=,cputime=", "-p", &pid])
        .output()
        .ok()?;
    parse_ps(&String::from_utf8_lossy(&output.stdout))
}

/// `rss` in KiB, then `cputime` as `[dd-][hh:]mm:ss[.cc]`.
fn parse_ps(text: &str) -> Option<Usage> {
    let mut fields = text.split_whitespace();
    let rss: u64 = fields.next()?.parse().ok()?;
    let cputime = fields.next()?;
    let (days, clock) = match cputime.split_once('-') {
        Some((days, clock)) => (days.parse::<f64>().ok()?, clock),
        None => (0.0, cputime),
    };
    let mut seconds = 0.0;
    for part in clock.split(':') {
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(Usage {
        cpu: Duration::try_from_secs_f64(days * 86_400.0 + seconds).ok()?,
        rss_bytes: rss * 1024,
    })
}

/// CPU time used between two readings, as a percent of one core over `wall`.
pub fn cpu_percent(from: &Usage, to: &Usage, wall: Duration) -> Option<f64> {
    (!wall.is_zero())
        .then(|| to.cpu.saturating_sub(from.cpu).as_secs_f64() / wall.as_secs_f64() * 100.0)
}

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
    let (first_frame, first_data) = start_times();
    let usage = process_usage();
    let (shows, show_p50, show_p95) = stats.show_latency();
    let run_cpu = usage
        .as_ref()
        .zip(STARTED.get())
        .and_then(|(usage, started)| {
            let wall = started.elapsed();
            (!wall.is_zero()).then(|| usage.cpu.as_secs_f64() / wall.as_secs_f64() * 100.0)
        });
    let sum = |counts: &std::collections::HashMap<String, u32>| counts.values().sum::<u32>();
    json!({
        "app": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpus": std::thread::available_parallelism().map(usize::from).unwrap_or(0),
        "start": { "first_frame_ms": first_frame, "first_data_ms": first_data },
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
        "bench": stats.bench.as_ref().map(|bench| json!({
            "terminals": bench.terminals,
            "shown": bench.shown,
            "hidden": bench.terminals.saturating_sub(bench.shown),
            "output": bench.output,
        })),
        "hidden_phase": stats.hidden_phase.as_ref().map(|phase| json!({
            "seconds": phase.seconds,
            "draws_per_second": phase.draws_per_second,
            "cpu_percent": phase.cpu_percent,
        })),
        "process": {
            // Over the whole run, as a percent of one core.
            "cpu_percent_run": run_cpu,
            "rss_bytes": usage.as_ref().map(|usage| usage.rss_bytes),
        },
        "link_reconnects": stats.link_lost,
        "terminal_reconnects": sum(&stats.reconnects),
        "skeletons": sum(&stats.skeletons),
        "show_latency": {
            "count": shows,
            "p50_ms": show_p50.as_ref().map(to_ms),
            "p95_ms": show_p95.as_ref().map(to_ms),
        },
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
        Some(home()?.join("Library/Logs/Natamux"))
    } else if cfg!(windows) {
        Some(
            PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
                .join("Natamux")
                .join("Logs"),
        )
    } else {
        Some(home()?.join(".local/state/natamux"))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::perf::stats::{BenchInfo, HiddenPhase};

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
    fn the_report_has_the_bench_numbers() {
        let mut stats = PerfStats::default();
        stats.bench = Some(BenchInfo {
            terminals: 30,
            shown: 6,
            output: "agent",
        });
        stats.hidden_phase = Some(HiddenPhase {
            seconds: 20.0,
            draws_per_second: 1.5,
            cpu_percent: None,
        });
        stats.link_lost = 1;
        stats.reconnect("901");
        stats.drew("902");
        stats.skeleton("902");
        stats.shown("903", Duration::from_millis(40));
        let report = report(&stats, Instant::now());
        assert_eq!(report["bench"]["terminals"], 30);
        assert_eq!(report["bench"]["hidden"], 24);
        assert_eq!(report["bench"]["output"], "agent");
        assert_eq!(report["hidden_phase"]["draws_per_second"], 1.5);
        assert_eq!(report["hidden_phase"]["cpu_percent"], Value::Null);
        assert_eq!(report["link_reconnects"], 1);
        assert_eq!(report["terminal_reconnects"], 1);
        assert_eq!(report["skeletons"], 1);
        assert_eq!(report["show_latency"]["count"], 1);
        assert_eq!(report["show_latency"]["p95_ms"], 40.0);
        let plain = super::report(&PerfStats::default(), Instant::now());
        assert_eq!(plain["bench"], Value::Null);
        assert_eq!(plain["hidden_phase"], Value::Null);
        assert_eq!(plain["show_latency"]["p50_ms"], Value::Null);
    }

    #[test]
    fn reads_ps_text_and_computes_cpu_percent() {
        let usage = parse_ps("  184320   1:02.50\n").unwrap();
        assert_eq!(usage.rss_bytes, 184_320 * 1024);
        assert_eq!(usage.cpu, Duration::from_millis(62_500));
        let long = parse_ps("10 1-02:03:04").unwrap();
        assert_eq!(
            long.cpu,
            Duration::from_secs(86_400 + 2 * 3600 + 3 * 60 + 4)
        );
        assert!(parse_ps("").is_none());
        assert!(parse_ps("12 soon").is_none());
        let later = Usage {
            cpu: usage.cpu + Duration::from_secs(2),
            rss_bytes: 0,
        };
        let percent = cpu_percent(&usage, &later, Duration::from_secs(20)).unwrap();
        assert!((percent - 10.0).abs() < 1e-9);
        assert!(cpu_percent(&usage, &later, Duration::ZERO).is_none());
    }

    #[test]
    fn the_start_times_are_kept_from_the_first_mark() {
        mark_first_data();
        mark_start();
        mark_first_frame();
        let (frame, _) = start_times();
        let frame = frame.unwrap();
        std::thread::sleep(Duration::from_millis(5));
        mark_first_frame();
        assert_eq!(start_times().0, Some(frame));
        let report = report(&PerfStats::default(), Instant::now());
        assert_eq!(report["start"]["first_frame_ms"], json!(frame));
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
