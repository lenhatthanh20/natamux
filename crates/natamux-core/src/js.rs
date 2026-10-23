//! The JavaScript rules the Node daemon's data follows: string lengths in UTF-16 units, `trim()`,
//! and `Date#toISOString`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `string.length`: UTF-16 code units, as zod's `max(400)` counts them.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// JS's WhiteSpace and LineTerminator: Rust's `char::is_whitespace` also takes U+0085 and leaves
/// out U+FEFF.
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `string.trim()`.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// `new Date(time).toISOString()`: UTC, milliseconds, `Z`.
pub fn iso_string(time: SystemTime) -> String {
    let millis = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_millis() as i64,
        Err(before) => -(before.duration().as_millis() as i64),
    };
    iso_from_millis(millis)
}

pub fn iso_now() -> String {
    iso_string(SystemTime::now())
}

/// `toISOString` of a time in milliseconds since 1970.
pub fn iso_from_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let in_day = millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let (hours, rest) = (in_day / 3_600_000, in_day % 3_600_000);
    let (minutes, rest) = (rest / 60_000, rest % 60_000);
    let (seconds, ms) = (rest / 1000, rest % 1000);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", -year)
    } else {
        format!("+{year:06}")
    };
    format!("{year}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{ms:03}Z")
}

/// `Date.parse` of an ISO time with `Z` (what the daemon writes): milliseconds since 1970.
pub fn parse_iso_millis(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let number = |from: usize, len: usize| -> Option<i64> {
        let part = text.get(from..from + len)?;
        part.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if bytes.len() < 20 || bytes.last() != Some(&b'Z') {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    let (hours, minutes, seconds) = (number(11, 2)?, number(14, 2)?, number(17, 2)?);
    let fraction = &text[19..text.len() - 1];
    let ms = match fraction.strip_prefix('.') {
        None if fraction.is_empty() => 0,
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            let first: String = digits.chars().chain("000".chars()).take(3).collect();
            first.parse().ok()?
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hours) * 60 + minutes) * 60_000 + seconds * 1000 + ms)
}

/// A time from milliseconds since 1970.
pub fn time_from_millis(millis: i64) -> SystemTime {
    if millis >= 0 {
        UNIX_EPOCH + Duration::from_millis(millis as u64)
    } else {
        UNIX_EPOCH - Duration::from_millis(millis.unsigned_abs())
    }
}

// Howard Hinnant's civil calendar algorithms.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_trims_as_javascript() {
        assert_eq!(utf16_len("a😀"), 3);
        assert_eq!(js_trim("\u{feff} a b\u{3000}\n"), "a b");
        assert_eq!(js_trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn writes_and_reads_iso_times() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            iso_from_millis(1_792_741_234_567),
            "2026-10-23T07:40:34.567Z"
        );
        assert_eq!(iso_from_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(
            parse_iso_millis("2026-10-23T07:40:34.567Z"),
            Some(1_792_741_234_567)
        );
        assert_eq!(
            parse_iso_millis("2026-10-23T07:40:34Z"),
            Some(1_792_741_234_000)
        );
        assert_eq!(
            parse_iso_millis("2026-10-23T07:40:34.5Z"),
            Some(1_792_741_234_500)
        );
        assert_eq!(parse_iso_millis("2026-10-23"), None);
    }
}
