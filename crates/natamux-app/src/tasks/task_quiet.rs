//! The quiet mark (`isQuiet`): a running task with no sign of life for a while may be stuck, so
//! it needs the user. An agent that waits for background work, such as a CI watch, is woken by
//! it, so such a task turns quiet only after the user's own limit, "Flag background work older
//! than" (T-0390), and never while that is off.

use natamux_proto::tasks::task::{Task, TaskState};

use crate::tasks::task_time::{format_clock, parse_iso_ms};

/// Ten minutes: a long command can keep an agent silent for a while without a problem.
pub const QUIET_AFTER_MS: i64 = 10 * 60 * 1000;

/// What a quiet check needs: the clock, and the limit for a task that waits for background work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attention {
    pub now_ms: i64,
    /// "Flag background work older than", in ms; None is off.
    pub background_ms: Option<i64>,
}

impl Attention {
    /// `minutes` is the setting; 0 is off.
    pub fn new(now_ms: i64, minutes: u32) -> Attention {
        let background_ms = (minutes > 0).then(|| i64::from(minutes).saturating_mul(60_000));
        Attention {
            now_ms,
            background_ms,
        }
    }
}

/// The clock alone: background work never turns quiet.
impl From<i64> for Attention {
    fn from(now_ms: i64) -> Attention {
        Attention {
            now_ms,
            background_ms: None,
        }
    }
}

/// When the task turns quiet, in epoch ms: its last sign of life plus its limit. None when it
/// cannot: it does not run, or it waits for background work with no limit set.
fn quiet_at(task: &Task, background_ms: Option<i64>) -> Option<i64> {
    if task.state != TaskState::Running {
        return None;
    }
    let limit = match task.background_since {
        None => QUIET_AFTER_MS,
        Some(_) => background_ms?,
    };
    Some(parse_iso_ms(&task.updated_at)?.saturating_add(limit))
}

/// A running task with no sign of life for its limit is probably stuck, hung or closed.
pub fn is_quiet(task: &Task, at: impl Into<Attention>) -> bool {
    let at = at.into();
    quiet_at(task, at.background_ms).is_some_and(|when| at.now_ms >= when)
}

/// A task whose background work had no sign of life for the user's limit (T-0390).
pub fn is_stale_background(task: &Task, at: impl Into<Attention>) -> bool {
    task.background_since.is_some() && is_quiet(task, at)
}

/// How long until the task turns quiet; None when it cannot, or already is.
pub(super) fn quiet_wait(task: &Task, at: Attention) -> Option<i64> {
    let wait = quiet_at(task, at.background_ms)? - at.now_ms;
    (wait > 0).then_some(wait)
}

/// How long until the next running task turns quiet: then it needs the user by the clock alone,
/// with no change of the data. None when no task can turn quiet.
pub fn next_quiet_ms<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    at: impl Into<Attention>,
) -> Option<i64> {
    let at = at.into();
    tasks
        .into_iter()
        .filter_map(|task| quiet_wait(task, at))
        .min()
}

/// The reason on the row of a stale task: "Waiting for Bash: pnpm test since 09:05". The entry is
/// the first one of the agent's line ("Waiting for Bash: pnpm test and 2 more").
pub fn stale_reason(activity: Option<&str>, since: &str, offset_minutes: i32) -> String {
    let what = activity
        .and_then(|line| line.strip_prefix("Waiting for "))
        .map_or("background work", first_entry);
    format!(
        "Waiting for {what} since {}",
        format_clock(since, offset_minutes)
    )
}

/// The reason on the row of a task, None when it is not stale. A blurred project hides the
/// entry, which may name a command or a file.
pub fn stale_label(
    task: &Task,
    line: Option<&str>,
    blurred: bool,
    at: Attention,
    offset_minutes: i32,
) -> Option<String> {
    let line = line.filter(|_| !blurred);
    is_stale_background(task, at).then(|| stale_reason(line, &task.updated_at, offset_minutes))
}

/// "Bash: a and 2 more" gives "Bash: a".
fn first_entry(list: &str) -> &str {
    let counted = list
        .strip_suffix(" more")
        .and_then(|rest| rest.rsplit_once(" and "));
    match counted {
        Some((first, count)) if count.chars().all(|c| c.is_ascii_digit()) && !count.is_empty() => {
            first
        }
        _ => list,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60 * 1000;

    fn waiting(updated_at: &str) -> Task {
        Task {
            state: TaskState::Running,
            updated_at: updated_at.into(),
            background_since: Some(updated_at.into()),
            ..Task::default()
        }
    }

    fn at(text: &str) -> i64 {
        parse_iso_ms(text).unwrap_or_default()
    }

    const SINCE: &str = "2026-11-26T09:00:00.000Z";

    #[test]
    fn background_work_never_turns_quiet_while_the_setting_is_off() {
        let task = waiting(SINCE);
        let later = at("2026-11-27T09:00:00.000Z");
        assert!(!is_quiet(&task, later));
        assert!(!is_quiet(&task, Attention::new(later, 0)));
        assert_eq!(next_quiet_ms([&task], later), None);
    }

    #[test]
    fn background_work_turns_quiet_at_each_limit_and_not_a_minute_before() {
        for minutes in [30, 60, 120, 240] {
            let task = waiting(SINCE);
            let limit = i64::from(minutes) * MINUTE;
            let start = at(SINCE);
            let before = Attention::new(start + limit - MINUTE, minutes);
            let exactly = Attention::new(start + limit, minutes);
            assert!(!is_quiet(&task, before), "{minutes}");
            assert!(is_quiet(&task, exactly), "{minutes}");
            assert!(is_stale_background(&task, exactly), "{minutes}");
            assert_eq!(next_quiet_ms([&task], before), Some(MINUTE), "{minutes}");
        }
    }

    #[test]
    fn a_task_without_background_work_keeps_the_ten_minutes() {
        let mut task = waiting(SINCE);
        task.background_since = None;
        let ten = Attention::new(at("2026-11-26T09:10:00.000Z"), 60);
        assert!(is_quiet(&task, ten));
        assert!(!is_stale_background(&task, ten));
    }

    #[test]
    fn only_a_running_task_turns_quiet() {
        let mut task = waiting(SINCE);
        task.state = TaskState::Idle;
        assert!(!is_quiet(
            &task,
            Attention::new(at("2026-11-27T09:00:00.000Z"), 30)
        ));
    }

    #[test]
    fn a_blurred_project_hides_the_entry_of_a_stale_row() {
        let task = waiting(SINCE);
        let late = Attention::new(at("2026-11-26T10:00:00.000Z"), 30);
        let line = Some("Waiting for Bash: deploy secret-app");
        assert_eq!(
            stale_label(&task, line, false, late, 0).as_deref(),
            Some("Waiting for Bash: deploy secret-app since 09:00")
        );
        assert_eq!(
            stale_label(&task, line, true, late, 0).as_deref(),
            Some("Waiting for background work since 09:00")
        );
        let early = Attention::new(at("2026-11-26T09:10:00.000Z"), 30);
        assert_eq!(stale_label(&task, line, false, early, 0), None);
    }

    #[test]
    fn the_reason_names_the_first_entry_and_the_time() {
        let line = Some("Waiting for Bash: pnpm test and 2 more");
        assert_eq!(
            stale_reason(line, "2026-11-26T09:05:00.000Z", 0),
            "Waiting for Bash: pnpm test since 09:05"
        );
        assert_eq!(
            stale_reason(Some("Waiting for subagent: review"), SINCE, 0),
            "Waiting for subagent: review since 09:00"
        );
        assert_eq!(
            stale_reason(None, SINCE, 0),
            "Waiting for background work since 09:00"
        );
        assert_eq!(
            stale_reason(Some("Waiting for a and b and more"), SINCE, 0),
            "Waiting for a and b and more since 09:00"
        );
    }
}
