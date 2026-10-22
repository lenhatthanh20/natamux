//! One time format for the whole app (`formatAge` and `formatExact` in `packages/web/src/lib/
//! tasks.ts`). Times are ISO 8601 in UTC. Rules never read the clock or the time zone: the caller
//! passes `now_ms` and the offset of the local zone, so the output is the same in every test.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgeStyle {
    Short,
    Long,
}

const WEEK_SECONDS: i64 = 7 * 86_400;
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date (year, month 1-12, day) of a day count.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Milliseconds since the epoch of `2026-10-08T09:00:00.000Z`. None when the text is not a time
/// in that shape (fraction and `Z` are optional).
pub fn parse_iso_ms(text: &str) -> Option<i64> {
    let text = text.strip_suffix('Z').unwrap_or(text);
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next().unwrap_or("0").parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let digits: String = fraction.chars().chain("000".chars()).take(3).collect();
    let millis: i64 = digits.parse().ok()?;
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// The local civil date and the seconds into that day.
fn local_parts(ms: i64, offset_minutes: i32) -> ((i64, i64, i64), i64) {
    let local = ms + i64::from(offset_minutes) * 60_000;
    let seconds = local.div_euclid(1000);
    (
        civil_from_days(seconds.div_euclid(86_400)),
        seconds.rem_euclid(86_400),
    )
}

/// Short is for rows and tables: now, 4m, 3h, 2d, then a date. Long is for sentences: just now,
/// 4m ago, 3h ago, 2d ago, then a date. A time that does not parse gives an empty text.
pub fn format_age(iso_time: &str, now_ms: i64, style: AgeStyle, offset_minutes: i32) -> String {
    let Some(then_ms) = parse_iso_ms(iso_time) else {
        return String::new();
    };
    let seconds = (now_ms - then_ms).div_euclid(1000).max(0);
    let long = style == AgeStyle::Long;
    if seconds < 60 {
        return if long { "just now" } else { "now" }.into();
    }
    if seconds >= WEEK_SECONDS {
        let ((year, month, day), _) = local_parts(then_ms, offset_minutes);
        let ((now_year, _, _), _) = local_parts(now_ms, offset_minutes);
        let name = MONTHS[(month - 1) as usize];
        return if year == now_year {
            format!("{name} {day}")
        } else {
            format!("{name} {day}, {year}")
        };
    }
    let (amount, unit) = if seconds < 3600 {
        (seconds / 60, "m")
    } else if seconds < 86_400 {
        (seconds / 3600, "h")
    } else {
        (seconds / 86_400, "d")
    };
    if long {
        format!("{amount}{unit} ago")
    } else {
        format!("{amount}{unit}")
    }
}

/// The exact local date and time for the tooltip of a relative time, like `Oct 8, 2026, 9:00:00 AM`.
pub fn format_exact(iso_time: &str, offset_minutes: i32) -> String {
    let Some(ms) = parse_iso_ms(iso_time) else {
        return String::new();
    };
    let ((year, month, day), seconds) = local_parts(ms, offset_minutes);
    let (hour, minute, second) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    let (shown, suffix) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    let name = MONTHS[(month - 1) as usize];
    format!("{name} {day}, {year}, {shown}:{minute:02}:{second:02} {suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        parse_iso_ms("2026-10-08T12:00:00.000Z").unwrap()
    }

    #[test]
    fn parses_a_time_and_rejects_text() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:01.500Z"), Some(1500));
        assert_eq!(
            parse_iso_ms("2026-10-08T09:00:00Z"),
            Some(1_791_450_000_000)
        );
        assert_eq!(parse_iso_ms("yesterday"), None);
    }

    #[test]
    fn shows_each_time_as_short_and_long() {
        let cases = [
            ("2026-10-08T11:59:30.000Z", "now", "just now"),
            ("2026-10-08T12:00:30.000Z", "now", "just now"),
            ("2026-10-08T11:58:30.000Z", "1m", "1m ago"),
            ("2026-10-08T11:00:01.000Z", "59m", "59m ago"),
            ("2026-10-08T11:00:00.000Z", "1h", "1h ago"),
            ("2026-10-07T13:00:00.000Z", "23h", "23h ago"),
            ("2026-10-05T12:00:00.000Z", "3d", "3d ago"),
            ("2026-10-02T12:00:00.000Z", "6d", "6d ago"),
            ("2026-10-01T12:00:00.000Z", "Oct 1", "Oct 1"),
            ("2025-10-03T12:00:00.000Z", "Oct 3, 2025", "Oct 3, 2025"),
        ];
        for (time, short, long) in cases {
            assert_eq!(format_age(time, now(), AgeStyle::Short, 0), short, "{time}");
            assert_eq!(format_age(time, now(), AgeStyle::Long, 0), long, "{time}");
        }
    }

    #[test]
    fn a_date_uses_the_local_offset() {
        // 23:30 UTC on Sep 30 is already Oct 1 at +60 minutes.
        let time = "2026-09-30T23:30:00.000Z";
        assert_eq!(format_age(time, now(), AgeStyle::Short, 0), "Sep 30");
        assert_eq!(format_age(time, now(), AgeStyle::Short, 60), "Oct 1");
    }

    #[test]
    fn gives_an_empty_text_for_a_time_that_does_not_parse() {
        assert_eq!(format_age("x", now(), AgeStyle::Short, 0), "");
        assert_eq!(format_exact("x", 0), "");
    }

    #[test]
    fn shows_the_exact_time() {
        assert_eq!(
            format_exact("2026-10-08T09:05:07.000Z", 0),
            "Oct 8, 2026, 9:05:07 AM"
        );
        assert_eq!(
            format_exact("2026-10-08T00:00:00.000Z", 0),
            "Oct 8, 2026, 12:00:00 AM"
        );
        assert_eq!(
            format_exact("2026-10-08T12:30:00.000Z", 0),
            "Oct 8, 2026, 12:30:00 PM"
        );
        assert_eq!(
            format_exact("2026-10-08T21:00:00.000Z", 0),
            "Oct 8, 2026, 9:00:00 PM"
        );
    }
}
