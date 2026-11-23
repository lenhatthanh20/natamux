//! `natamux security [--json]`: the security mode and what each key of Strict mode blocks
//! (`GET /security`). Read only: the settings change in the app (`PUT /security`).

use natamux_client::Method;
use serde_json::Value;

use crate::daemon::{DaemonClient, Transport};

/// `words`: what follows `security`; the command takes none.
pub fn parse(words: &[String]) -> Result<(), String> {
    match words.first() {
        None => Ok(()),
        Some(extra) => Err(format!("Unexpected argument: {extra}")),
    }
}

pub async fn run<T: Transport>(json: bool, client: &DaemonClient<T>) -> Result<String, String> {
    let body = client
        .call(Method::GET, "/security", None::<&()>)
        .await?
        .unwrap_or(Value::Null);
    if json {
        let text = serde_json::to_string_pretty(&body).map_err(|error| error.to_string())?;
        return Ok(format!("{text}\n"));
    }
    Ok(text_of(&body))
}

/// The answer as lines: the mode, a warning, then each key with what it blocks.
pub fn text_of(body: &Value) -> String {
    let strict = body["strict"].as_bool().unwrap_or(false);
    let mut text = format!(
        "Security mode: {}\n",
        if strict { "Strict" } else { "Personal" }
    );
    if let Some(warning) = body["warning"].as_str() {
        text.push_str(&format!("Warning: {warning}\n"));
    }
    if let Some(keys) = body["keys"].as_object() {
        for (key, state) in keys {
            let value = state["value"].as_str().unwrap_or_default();
            let line = match state["reason"].as_str() {
                Some(reason) if state["blocked"] == true => {
                    format!("  {key}: {value} (blocked: {reason})\n")
                }
                _ => format!("  {key}: {value}\n"),
            };
            text.push_str(&line);
        }
    }
    if let Some(ignored) = body["ignored"].as_array().filter(|list| !list.is_empty()) {
        let names: Vec<&str> = ignored.iter().filter_map(Value::as_str).collect();
        text.push_str(&format!(
            "Ignored (looser than Strict): {}\n",
            names.join(", ")
        ));
    }
    text
}
