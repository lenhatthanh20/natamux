//! The implicit types of js-yaml 3's safe schema (`lib/js-yaml/type/*.js`): what a plain scalar
//! means when it is read, and which strings the writer must quote.

use natamux_core::js::{days_from_civil, iso_from_millis};
use serde_json::{Number, Value};

/// A plain scalar as js-yaml reads it. A timestamp becomes its ISO text (`Date#toISOString`),
/// which is what the task reader makes of a Date at the top level; `Timestamp` keeps that apart.
#[derive(Clone, Debug, PartialEq)]
pub enum Plain {
    Value(Value),
    Timestamp(String),
    Merge,
}

/// The implicit types in js-yaml's order: null, bool, int, float, timestamp, merge.
pub fn resolve_plain(text: &str) -> Plain {
    if is_null(text) {
        return Plain::Value(Value::Null);
    }
    if is_bool(text) {
        return Plain::Value(Value::Bool(matches!(text, "true" | "True" | "TRUE")));
    }
    if is_int(text) {
        return Plain::Value(number(construct_int(text)));
    }
    if is_float(text) {
        return Plain::Value(number(construct_float(text)));
    }
    if let Some(millis) = timestamp_millis(text) {
        return Plain::Timestamp(iso_from_millis(millis));
    }
    if text == "<<" {
        return Plain::Merge;
    }
    Plain::Value(Value::String(text.to_string()))
}

/// A string the writer must quote, as another type would read it.
pub fn is_ambiguous(text: &str) -> bool {
    is_null(text)
        || is_bool(text)
        || is_int(text)
        || is_float(text)
        || timestamp_parts(text).is_some()
        || text == "<<"
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0
        && value.abs() <= 9_007_199_254_740_992.0
        && !(value == 0.0 && value.is_sign_negative())
    {
        return Value::Number(Number::from(value as i64));
    }
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

fn is_null(text: &str) -> bool {
    matches!(text, "~" | "null" | "Null" | "NULL" | "")
}

fn is_bool(text: &str) -> bool {
    matches!(text, "true" | "True" | "TRUE" | "false" | "False" | "FALSE")
}

/// `resolveYamlInteger`.
pub fn is_int(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = 0;
    if matches!(bytes[0], b'-' | b'+') {
        index = 1;
    }
    let digits_until = |from: usize, ok: fn(u8) -> bool| -> bool {
        let rest = &bytes[from.min(bytes.len())..];
        let has = rest.iter().any(|b| *b != b'_');
        has && rest.iter().all(|b| *b == b'_' || ok(*b)) && rest.last() != Some(&b'_')
    };
    match bytes.get(index) {
        None => false,
        Some(b'0') => {
            if index + 1 == bytes.len() {
                return true;
            }
            match bytes[index + 1] {
                b'b' => digits_until(index + 2, |b| b == b'0' || b == b'1'),
                b'x' => digits_until(index + 2, |b| b.is_ascii_hexdigit()),
                _ => digits_until(index + 1, |b| (b'0'..=b'7').contains(&b)),
            }
        }
        Some(b'_') => false,
        Some(_) => {
            let rest = &bytes[index..];
            let colon = rest.iter().position(|b| *b == b':');
            let head = &rest[..colon.unwrap_or(rest.len())];
            let has = head.iter().any(u8::is_ascii_digit);
            let ok = head.iter().all(|b| *b == b'_' || b.is_ascii_digit());
            // js-yaml looks at the last character it read, which is the ':' when there is one.
            if !has || !ok || (colon.is_none() && head.last() == Some(&b'_')) {
                return false;
            }
            match colon {
                None => true,
                Some(at) => sexagesimal_tail(&rest[at..], false),
            }
        }
    }
}

/// `(:[0-5]?[0-9])+` to the end (`with_fraction`: then `\.[0-9_]*`).
fn sexagesimal_tail(bytes: &[u8], with_fraction: bool) -> bool {
    let mut index = 0;
    let mut parts = 0;
    while index < bytes.len() && bytes[index] == b':' {
        let digits = bytes[index + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        let part = &bytes[index + 1..index + 1 + digits];
        let ok = match part {
            [d] => d.is_ascii_digit(),
            [a, _] => (b'0'..=b'5').contains(a),
            _ => false,
        };
        if !ok {
            return false;
        }
        parts += 1;
        index += 1 + digits;
    }
    if parts == 0 {
        return false;
    }
    if !with_fraction {
        return index == bytes.len();
    }
    bytes.get(index) == Some(&b'.')
        && bytes[index + 1..]
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b'_')
}

fn construct_int(text: &str) -> f64 {
    let cleaned = text.replace('_', "");
    let (sign, value) = match cleaned.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    if value == "0" {
        return 0.0;
    }
    let radix = |digits: &str, base: u32| {
        digits
            .chars()
            .filter_map(|c| c.to_digit(base))
            .fold(0.0, |sum, digit| sum * f64::from(base) + f64::from(digit))
    };
    if let Some(rest) = value.strip_prefix("0b") {
        return sign * radix(rest, 2);
    }
    if let Some(rest) = value.strip_prefix("0x") {
        return sign * radix(rest, 16);
    }
    if value.starts_with('0') {
        return sign * radix(value, 8);
    }
    if value.contains(':') {
        let total = value
            .split(':')
            .rev()
            .enumerate()
            .fold(0.0, |sum, (place, part)| {
                sum + radix(part, 10) * 60f64.powi(place as i32)
            });
        return sign * total;
    }
    sign * radix(value, 10)
}

/// `YAML_FLOAT_PATTERN`, and no `_` at the end.
pub fn is_float(text: &str) -> bool {
    if text.ends_with('_') {
        return false;
    }
    let bytes = text.as_bytes();
    let signed = bytes.first().is_some_and(|b| matches!(b, b'-' | b'+'));
    let body = if signed { &bytes[1..] } else { bytes };
    let special = matches!(body, b".inf" | b".Inf" | b".INF");
    if special || (!signed && matches!(bytes, b".nan" | b".NaN" | b".NAN")) {
        return true;
    }
    if !signed && bytes.first() == Some(&b'.') {
        // `\.[0-9_]+(?:[eE][-+]?[0-9]+)?`
        let digits = bytes[1..]
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b'_')
            .count();
        return digits > 0 && exponent(&bytes[1 + digits..]);
    }
    decimal(body) || sexagesimal_float(body)
}

/// `(?:0|[1-9][0-9_]*)(?:\.[0-9_]*)?(?:[eE][-+]?[0-9]+)?`.
fn decimal(body: &[u8]) -> bool {
    let whole = match body.first() {
        Some(b'0') => 1,
        Some(b'1'..=b'9') => {
            1 + body[1..]
                .iter()
                .take_while(|b| b.is_ascii_digit() || **b == b'_')
                .count()
        }
        _ => return false,
    };
    let mut rest = &body[whole..];
    if rest.first() == Some(&b'.') {
        let fraction = rest[1..]
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b'_')
            .count();
        rest = &rest[1 + fraction..];
    }
    exponent(rest)
}

fn exponent(rest: &[u8]) -> bool {
    match rest.first() {
        None => true,
        Some(b'e' | b'E') => {
            let digits = match rest.get(1) {
                Some(b'-' | b'+') => &rest[2..],
                _ => &rest[1..],
            };
            !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
        }
        _ => false,
    }
}

/// `[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*`.
fn sexagesimal_float(body: &[u8]) -> bool {
    if !body.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let head = body
        .iter()
        .take_while(|b| b.is_ascii_digit() || **b == b'_')
        .count();
    sexagesimal_tail(&body[head..], true)
}

fn construct_float(text: &str) -> f64 {
    let value = text.replace('_', "").to_lowercase();
    let sign = if value.starts_with('-') { -1.0 } else { 1.0 };
    let value = value.trim_start_matches(['-', '+']);
    match value {
        ".inf" => sign * f64::INFINITY,
        ".nan" => f64::NAN,
        _ if value.contains(':') => {
            let total = value
                .split(':')
                .rev()
                .enumerate()
                .fold(0.0, |sum, (place, part)| {
                    sum + parse_float_prefix(part) * 60f64.powi(place as i32)
                });
            sign * total
        }
        _ => sign * parse_float_prefix(value),
    }
}

/// `parseFloat`: the longest number at the start.
fn parse_float_prefix(text: &str) -> f64 {
    let text = if text.starts_with('.') {
        format!("0{text}")
    } else {
        text.to_string()
    };
    (1..=text.len())
        .rev()
        .find_map(|end| text.get(..end).and_then(|part| part.parse::<f64>().ok()))
        .unwrap_or(f64::NAN)
}

/// year, month, day, and when given: hour, minute, second, fraction digits, offset in minutes.
type Parts = (i64, i64, i64, Option<(i64, i64, i64, String, i64)>);

/// `YAML_DATE_REGEXP` or `YAML_TIMESTAMP_REGEXP`.
fn timestamp_parts(text: &str) -> Option<Parts> {
    let b = text.as_bytes();
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = b.get(from..to)?;
        (!part.is_empty() && part.iter().all(u8::is_ascii_digit))
            .then(|| text[from..to].parse().ok())?
    };
    let year = num(0, 4)?;
    if b.get(4) != Some(&b'-') {
        return None;
    }
    if b.len() == 10 && b[7] == b'-' {
        return Some((year, num(5, 7)?, num(8, 10)?, None));
    }
    let digits_at = |from: usize, max: usize| -> usize {
        b[from.min(b.len())..]
            .iter()
            .take(max)
            .take_while(|c| c.is_ascii_digit())
            .count()
    };
    let mut at = 5;
    let month_len = digits_at(at, 2);
    let month = num(at, at + month_len)?;
    at += month_len;
    if b.get(at) != Some(&b'-') {
        return None;
    }
    at += 1;
    let day_len = digits_at(at, 2);
    let day = num(at, at + day_len)?;
    at += day_len;
    match b.get(at) {
        Some(b'T' | b't') => at += 1,
        Some(b' ' | b'\t') => {
            at += b[at..]
                .iter()
                .take_while(|c| matches!(c, b' ' | b'\t'))
                .count()
        }
        _ => return None,
    }
    let hour_len = digits_at(at, 2);
    let hour = num(at, at + hour_len)?;
    at += hour_len;
    let mut pair = || -> Option<i64> {
        if b.get(at) != Some(&b':') || digits_at(at + 1, 2) != 2 {
            return None;
        }
        at += 3;
        num(at - 2, at)
    };
    let minute = pair()?;
    let second = pair()?;
    let (fraction, offset) = fraction_and_zone(text, at)?;
    Some((
        year,
        month,
        day,
        Some((hour, minute, second, fraction, offset)),
    ))
}

/// `(?:\.([0-9]*))?(?:[ \t]*(Z|[-+]H[H][:MM]))?$` from `at`: the fraction digits and the offset.
fn fraction_and_zone(text: &str, mut at: usize) -> Option<(String, i64)> {
    let b = text.as_bytes();
    let mut fraction = String::new();
    if b.get(at) == Some(&b'.') {
        let len = b[at + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        fraction = text[at + 1..at + 1 + len].to_string();
        at += 1 + len;
    }
    let zone_start = at
        + b[at..]
            .iter()
            .take_while(|c| matches!(c, b' ' | b'\t'))
            .count();
    let offset = if zone_start == b.len() {
        if at != b.len() {
            return None;
        }
        0
    } else {
        zone_offset(&text[zone_start..])?
    };
    Some((fraction, offset))
}

/// `Z` or `[-+]H[H][:MM]`, in minutes.
fn zone_offset(zone: &str) -> Option<i64> {
    if zone == "Z" {
        return Some(0);
    }
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let rest = &zone[1..];
    let (hours, minutes) = rest.split_once(':').unwrap_or((rest, "00"));
    let ok = (1..=2).contains(&hours.len())
        && minutes.len() == 2
        && hours
            .bytes()
            .chain(minutes.bytes())
            .all(|c| c.is_ascii_digit());
    ok.then(|| {
        sign * (hours.parse::<i64>().unwrap_or(0) * 60 + minutes.parse::<i64>().unwrap_or(0))
    })
}

/// `constructYamlTimestamp`, in milliseconds since 1970 (`Date.UTC` rolls over large parts).
fn timestamp_millis(text: &str) -> Option<i64> {
    let (year, month, day, time) = timestamp_parts(text)?;
    // Date.UTC reads the years 0 to 99 as 1900 to 1999.
    let year = if (0..=99).contains(&year) {
        year + 1900
    } else {
        year
    };
    let months = year * 12 + month - 1;
    let days = days_from_civil(months.div_euclid(12), months.rem_euclid(12) + 1, 1) + day - 1;
    let Some((hour, minute, second, fraction, offset)) = time else {
        return Some(days * 86_400_000);
    };
    let ms: i64 = format!("{fraction:0<3}")[..3].parse().unwrap_or(0);
    let local = ((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + ms;
    Some(local - offset * 60_000)
}

#[cfg(test)]
#[path = "yaml_types_tests.rs"]
mod tests;
