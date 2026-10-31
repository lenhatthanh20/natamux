//! The body of `PUT /custom-agents`: the whole list of the user's custom agents.

use natamux_proto::agents::custom::{CustomAgent, MAX_CUSTOM_AGENTS, OTHER_BASE};
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;
use serde_json::{Value, json};

use crate::formats::is_slug;
use crate::zod::{
    Issue, Issues, Length, Pattern, Rule, ZodError, array, at, object, one_of, strict, string,
};

const ID: [Rule; 2] = [
    Rule::Regex(Pattern {
        test: is_slug,
        source: "/^[a-z0-9]+(-[a-z0-9]+)*$/",
        message: Some("An agent id is lower case words joined by \"-\""),
    }),
    Rule::Max(32),
];

fn is_one_line(text: &str) -> bool {
    !text.contains(['\n', '\r'])
}

const COMMAND: [Rule; 4] = [
    Rule::Trim,
    Rule::Min(1),
    Rule::Max(500),
    Rule::Regex(Pattern {
        test: is_one_line,
        source: "/^[^\\r\\n]*$/",
        message: Some("The command is one line"),
    }),
];

fn agent(value: &Value, path: &[Value], issues: &mut Issues) -> Option<CustomAgent> {
    let map = object(Some(value), path, issues)?;
    strict(map, &["id", "base", "command"], path, issues);
    let id = string(map.get("id"), &at(path, "id"), &ID, issues);
    let mut bases: Vec<&str> = LAUNCHABLE_AGENTS.to_vec();
    bases.push(OTHER_BASE);
    let base = one_of(map.get("base"), &at(path, "base"), &bases, issues);
    let command = string(map.get("command"), &at(path, "command"), &COMMAND, issues);
    Some(CustomAgent {
        id: id?,
        base: base?,
        command: command?,
    })
}

/// A built-in id, `other`, or the id of an earlier row.
fn is_taken(id: &str, earlier: &[CustomAgent]) -> bool {
    LAUNCHABLE_AGENTS.contains(&id) || id == OTHER_BASE || earlier.iter().any(|row| row.id == id)
}

/// `{ agents: [...] }`, checked: each id is new (not a built-in, not twice), each base is known.
pub fn parse_custom_agents(value: Option<&Value>) -> Result<Vec<CustomAgent>, ZodError> {
    let mut issues = Issues::default();
    let mut parsed = None;
    if let Some(map) = object(value, &[], &mut issues) {
        strict(map, &["agents"], &[], &mut issues);
        let length = Length {
            min: None,
            max: Some(MAX_CUSTOM_AGENTS),
        };
        let path = [json!("agents")];
        parsed = array(map.get("agents"), &path, length, &mut issues, agent);
        let rows = parsed.as_deref().unwrap_or_default();
        for (index, each) in rows.iter().enumerate() {
            if is_taken(&each.id, &rows[..index]) {
                let path = at(&at(&path, index), "id");
                let message = format!("The agent id \"{}\" is taken", each.id);
                issues.push(Issue::custom(&path, &message));
            }
        }
    }
    issues.into_result(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: Value) -> Result<Vec<CustomAgent>, String> {
        parse_custom_agents(Some(&value)).map_err(|error| error.describe())
    }

    fn row(id: &str, base: &str, command: &str) -> Value {
        json!({ "id": id, "base": base, "command": command })
    }

    #[test]
    fn takes_a_claude_variant_and_another_cli() {
        let body = json!({ "agents": [
            row("claude-qwen", "claude-code", "  cq  "),
            row("my-agent", "other", "my-agent --fast"),
        ]});
        let agents = parse(body).unwrap();
        assert_eq!(agents[0].command, "cq");
        assert_eq!(agents[1].base, "other");
        assert_eq!(parse(json!({ "agents": [] })).unwrap(), vec![]);
    }

    #[test]
    fn refuses_a_bad_row() {
        for bad in [
            row("Claude Qwen", "claude-code", "cq"),
            row("claude-code", "claude-code", "cq"),
            row("other", "other", "x"),
            row("qwen", "aider", "cq"),
            row("qwen", "claude-code", "   "),
            row("qwen", "claude-code", "cq\nrm -rf /"),
            row(&"a".repeat(33), "claude-code", "cq"),
            row("qwen", "claude-code", &"a".repeat(501)),
            json!({ "id": "qwen", "base": "claude-code", "command": "cq", "env": {} }),
        ] {
            assert!(parse(json!({ "agents": [bad.clone()] })).is_err(), "{bad}");
        }
    }

    #[test]
    fn refuses_one_id_twice_and_too_many_rows() {
        let body = json!({ "agents": [row("q", "codex", "a"), row("q", "codex", "b")] });
        assert!(parse(body).unwrap_err().contains("agents.1.id"));
        let rows: Vec<Value> = (0..=MAX_CUSTOM_AGENTS)
            .map(|n| row(&format!("a{n}"), "codex", "x"))
            .collect();
        assert!(parse(json!({ "agents": rows })).is_err());
        assert!(parse(json!([])).is_err());
    }
}
