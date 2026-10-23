//! JSON as JavaScript reads and writes it, so the daemon's answers, logs and index rows are the
//! bytes the Node daemon writes. `serde_json` differs: it writes `1.0` for a float and keeps the
//! order of keys that look like numbers, which a JS object puts first.

use serde_json::{Map, Number, Value};

/// `JSON.parse`: numbers become doubles, and the keys of an object take a JS object's order.
pub fn parse_js(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str::<Value>(text).map(js_value)
}

/// A value as a JS object holds it: numbers as doubles, keys in JS order.
pub fn js_value(value: Value) -> Value {
    match value {
        Value::Number(number) => js_number(&number),
        Value::Array(items) => Value::Array(items.into_iter().map(js_value).collect()),
        Value::Object(map) => Value::Object(js_ordered(map)),
        other => other,
    }
}

fn js_number(number: &Number) -> Value {
    if number.is_f64() {
        return Value::Number(number.clone());
    }
    let float = number.as_f64().unwrap_or(0.0);
    // A double holds every integer up to 2^53; above that JS keeps the nearest double.
    if float.abs() <= 9_007_199_254_740_992.0
        && let Some(int) = number.as_i64()
    {
        return Value::Number(int.into());
    }
    Number::from_f64(float).map_or(Value::Null, Value::Number)
}

/// Array-index keys ("0", "17") first in number order, then the others in the order they came.
pub fn js_ordered(map: Map<String, Value>) -> Map<String, Value> {
    if !map.keys().any(|key| array_index(key).is_some()) {
        return map
            .into_iter()
            .map(|(key, value)| (key, js_value(value)))
            .collect();
    }
    let mut indexed: Vec<(u32, String, Value)> = Vec::new();
    let mut named: Vec<(String, Value)> = Vec::new();
    for (key, value) in map {
        match array_index(&key) {
            Some(index) => indexed.push((index, key, js_value(value))),
            None => named.push((key, js_value(value))),
        }
    }
    indexed.sort_by_key(|(index, _, _)| *index);
    indexed
        .into_iter()
        .map(|(_, key, value)| (key, value))
        .chain(named)
        .collect()
}

/// The keys of a JS object in the order JS gives them.
pub fn js_keys(map: &Map<String, Value>) -> Vec<&String> {
    let mut indexed: Vec<(u32, &String)> = Vec::new();
    let mut named: Vec<&String> = Vec::new();
    for key in map.keys() {
        match array_index(key) {
            Some(index) => indexed.push((index, key)),
            None => named.push(key),
        }
    }
    indexed.sort_by_key(|(index, _)| *index);
    indexed
        .into_iter()
        .map(|(_, key)| key)
        .chain(named)
        .collect()
}

/// A canonical array index: "0" to "4294967294", no leading zero.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    if !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|index| *index != u32::MAX)
}

/// `JSON.stringify(value)`.
pub fn to_js_string(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

/// `JSON.stringify` of a serializable value.
pub fn to_js_string_of<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(value) => to_js_string(&value),
        Err(_) => "null".into(),
    }
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&number_text(number)),
        Value::String(text) => write_string(out, text),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, key) in js_keys(map).into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_value(out, &map[key.as_str()]);
            }
            out.push('}');
        }
    }
}

fn number_text(number: &Number) -> String {
    if let Some(int) = number.as_i64()
        && (int as f64).abs() <= 9_007_199_254_740_992.0
    {
        return int.to_string();
    }
    number_to_js(number.as_f64().unwrap_or(0.0))
}

/// `Number.prototype.toString()` of a double; `null` for NaN and infinities, as JSON writes them.
pub fn number_to_js(value: f64) -> String {
    if !value.is_finite() {
        return "null".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    // `{:e}` gives the shortest digits that read back the same, as JS's algorithm does.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let n = exponent.parse::<i32>().unwrap_or(0) + 1;
    let k = digits.len() as i32;
    let sign = if value < 0.0 { "-" } else { "" };
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let exp = n - 1;
        let exp_sign = if exp < 0 { '-' } else { '+' };
        let head = if k == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{head}e{exp_sign}{}", exp.abs())
    };
    format!("{sign}{body}")
}

/// A JSON string as `JSON.stringify` writes it.
pub fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_numbers_as_javascript() {
        for (value, text) in [
            (1.0, "1"),
            (-0.0, "0"),
            (1.5, "1.5"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (123.456, "123.456"),
            (-2.5e30, "-2.5e+30"),
            (f64::NAN, "null"),
        ] {
            assert_eq!(number_to_js(value), text, "{value}");
        }
    }

    #[test]
    fn reads_and_writes_as_json_parse_and_stringify() {
        let value =
            parse_js(r#"{"b":1.0,"2":[1e2,12345678901234567890],"a":"x\u0001\"","1":null}"#)
                .unwrap();
        assert_eq!(
            to_js_string(&value),
            r#"{"1":null,"2":[100,12345678901234567000],"b":1,"a":"x\u0001\""}"#
        );
        assert_eq!(to_js_string(&json!({"01": 1, "0": 2})), r#"{"0":2,"01":1}"#);
        assert_eq!(
            to_js_string(&json!("tab\tline\u{2028}")),
            "\"tab\\tline\u{2028}\""
        );
    }

    #[test]
    fn keeps_the_first_place_of_a_key_given_twice() {
        let value = parse_js(r#"{"a":1,"b":2,"a":3}"#).unwrap();
        assert_eq!(to_js_string(&value), r#"{"a":3,"b":2}"#);
    }
}
