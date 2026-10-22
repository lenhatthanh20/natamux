//! Port of `packages/web/src/features/app/background-jobs.ts`: the footer's words about the jobs
//! that run now. The clock is a parameter (`now_ms`, Unix milliseconds).

use natamux_proto::tasks::job::Job;

/// What the footer says about the jobs. None: no job, so it shows nothing.
pub fn jobs_label(jobs: &[Job]) -> Option<String> {
    match jobs {
        [] => None,
        [only] => Some(match &only.task_id {
            None => only.label.clone(),
            Some(task_id) => format!("{} · {task_id}", only.label),
        }),
        many => Some(format!("{} background jobs", many.len())),
    }
}

// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix milliseconds of the daemon's timestamps ("2026-10-21T09:00:00.000Z"). Only UTC. None for
/// anything else.
pub fn parse_utc_ms(text: &str) -> Option<i64> {
    let body = text.strip_suffix('Z')?;
    let (date, time) = body.split_once('T')?;
    let mut d = date.split('-');
    let (year, month, day) = (d.next()?, d.next()?, d.next()?);
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut t = clock.split(':');
    let (hour, minute, second) = (t.next()?, t.next()?, t.next()?);
    let num = |s: &str| s.parse::<i64>().ok();
    let millis = format!("{fraction:0<3}");
    let millis = num(millis.get(..3)?)?;
    let days = days_from_civil(num(year)?, num(month)?, num(day)?);
    let seconds = days * 86_400 + num(hour)? * 3600 + num(minute)? * 60 + num(second)?;
    Some(seconds * 1000 + millis)
}

/// How long a job has run: 12s, 3m, 2h. A start time that cannot be read counts as just now.
pub fn format_elapsed(started_at: &str, now_ms: i64) -> String {
    let started = parse_utc_ms(started_at).unwrap_or(now_ms);
    let seconds = ((now_ms - started) / 1000).max(0);
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m", seconds / 60),
        _ => format!("{}h", seconds / 3600),
    }
}

/// A row of the jobs menu. A row opens its task; a job about no one task has nothing to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRow {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub disabled: bool,
    /// What a click opens.
    pub open_task: Option<String>,
}

pub fn job_rows(jobs: &[Job], now_ms: i64) -> Vec<JobRow> {
    jobs.iter()
        .map(|job| {
            let elapsed = format_elapsed(&job.started_at, now_ms);
            JobRow {
                id: job.id.clone(),
                label: job.label.clone(),
                hint: match &job.task_id {
                    None => elapsed,
                    Some(task_id) => format!("{task_id} · {elapsed}"),
                },
                disabled: job.task_id.is_none(),
                open_task: job.task_id.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, label: &str, task_id: Option<&str>, started_at: &str) -> Job {
        Job {
            id: id.into(),
            kind: "workspace".into(),
            label: label.into(),
            task_id: task_id.map(String::from),
            started_at: started_at.into(),
        }
    }

    fn push() -> Job {
        job(
            "job-1",
            "Pushing",
            Some("T-0135"),
            "2026-10-21T09:00:00.000Z",
        )
    }

    fn importing() -> Job {
        job(
            "job-2",
            "Importing issues",
            None,
            "2026-10-21T09:02:00.000Z",
        )
    }

    fn at(text: &str) -> i64 {
        parse_utc_ms(text).expect("a time")
    }

    #[test]
    fn is_null_with_no_job() {
        assert_eq!(jobs_label(&[]), None);
    }

    #[test]
    fn names_one_job_and_its_task() {
        assert_eq!(jobs_label(&[push()]).as_deref(), Some("Pushing · T-0135"));
        assert_eq!(
            jobs_label(&[importing()]).as_deref(),
            Some("Importing issues")
        );
    }

    #[test]
    fn counts_two_jobs_or_more() {
        assert_eq!(
            jobs_label(&[push(), importing()]).as_deref(),
            Some("2 background jobs")
        );
    }

    #[test]
    fn formats_elapsed_time() {
        let start = "2026-10-21T09:00:00.000Z";
        for (now, text) in [
            ("2026-10-21T09:00:12.000Z", "12s"),
            ("2026-10-21T09:03:30.000Z", "3m"),
            ("2026-10-21T11:10:00.000Z", "2h"),
            // A clock a little behind the daemon's.
            ("2026-10-21T08:59:59.000Z", "0s"),
        ] {
            assert_eq!(format_elapsed(start, at(now)), text, "at {now}");
        }
    }

    #[test]
    fn parses_timestamps() {
        assert_eq!(parse_utc_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc_ms("1970-01-02T00:00:01.5Z"), Some(86_401_500));
        assert_eq!(
            parse_utc_ms("2000-03-01T00:00:00.000Z"),
            Some(951_868_800_000)
        );
        assert_eq!(parse_utc_ms("soon"), None);
        assert_eq!(parse_utc_ms("2026-10-21T09:00:00+02:00"), None);
    }

    #[test]
    fn gives_each_job_a_row_with_its_task_and_time_and_opens_the_task() {
        let rows = job_rows(&[push(), importing()], at("2026-10-21T09:03:00.000Z"));
        assert_eq!(rows[0].id, "job-1");
        assert_eq!(rows[0].label, "Pushing");
        assert_eq!(rows[0].hint, "T-0135 · 3m");
        assert!(!rows[0].disabled);
        assert_eq!(rows[0].open_task.as_deref(), Some("T-0135"));
        assert_eq!(rows[1].label, "Importing issues");
        assert_eq!(rows[1].hint, "1m");
        assert!(rows[1].disabled);
        assert_eq!(rows[1].open_task, None);
    }
}
