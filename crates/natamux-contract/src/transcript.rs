//! What a scenario saw on one daemon, made comparable: shell ids and tickets become
//! placeholders. Two transcripts must be equal, step by step, but for the differences a scenario
//! allows by name, each with its reason.

use std::fmt::Write as _;
use std::io::Write as _;

use serde_json::Value;

pub struct Transcript {
    pub daemon: &'static str,
    pub steps: Vec<(String, Value)>,
    shells: Vec<String>,
    /// Texts each daemon has its own of, such as the id of its twin task, and their placeholder.
    aliases: Vec<(String, String)>,
}

/// A step whose answers may differ between the daemons, and why (docs/rust-phase4.md).
pub struct Allowed {
    pub step: &'static str,
    pub reason: &'static str,
}

impl Transcript {
    pub fn new(daemon: &'static str) -> Transcript {
        Transcript {
            daemon,
            steps: Vec::new(),
            shells: Vec::new(),
            aliases: Vec::new(),
        }
    }

    /// Every later note has `text` as `placeholder`.
    pub fn alias(&mut self, text: &str, placeholder: &str) {
        self.aliases.push((text.into(), placeholder.into()));
    }

    pub fn note(&mut self, step: &str, value: Value) {
        let value = self.normalize(value);
        self.steps.push((step.to_string(), value));
    }

    fn normalize(&mut self, value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        let value = match (key.as_str(), value) {
                            ("shellId", Value::String(id)) => Value::String(self.shell(id)),
                            ("ticket", Value::String(_)) => Value::String("<ticket>".into()),
                            (_, value) => self.normalize(value),
                        };
                        (key, value)
                    })
                    .collect(),
            ),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|item| self.normalize(item)).collect())
            }
            Value::String(text) => Value::String(
                self.aliases
                    .iter()
                    .fold(text, |text, (from, to)| text.replace(from.as_str(), to)),
            ),
            other => other,
        }
    }

    fn shell(&mut self, id: String) -> String {
        let index = match self.shells.iter().position(|known| *known == id) {
            Some(index) => index,
            None => {
                self.shells.push(id);
                self.shells.len() - 1
            }
        };
        format!("<shell {}>", index + 1)
    }
}

/// Panics with every step that differs and is not allowed. Allowed differences are printed and
/// written to the report file, when NATAMUX_CONTRACT_REPORT names one.
pub fn compare(scenario: &str, node: &Transcript, rust: &Transcript, allowed: &[Allowed]) {
    let mut errors = String::new();
    let mut report = String::new();
    let labels = |transcript: &Transcript| -> Vec<String> {
        transcript
            .steps
            .iter()
            .map(|(step, _)| step.clone())
            .collect()
    };
    if labels(node) != labels(rust) {
        let _ = writeln!(
            errors,
            "steps differ:\n  node: {:?}\n  rust: {:?}",
            labels(node),
            labels(rust)
        );
    }
    for ((step, left), (_, right)) in node.steps.iter().zip(&rust.steps) {
        if left == right {
            continue;
        }
        match allowed.iter().find(|allowed| allowed.step == step) {
            Some(allowed) => {
                let _ = writeln!(report, "| {scenario} | {step} | {} |", allowed.reason);
            }
            None => {
                let _ = writeln!(errors, "{step}:\n  node: {left}\n  rust: {right}");
            }
        }
    }
    print!("{report}");
    if let Ok(path) = std::env::var("NATAMUX_CONTRACT_REPORT") {
        let line = if errors.is_empty() {
            format!(
                "| {scenario} | all steps | {} |\n",
                if report.is_empty() {
                    "equal"
                } else {
                    "equal but the rows above"
                }
            )
        } else {
            format!("| {scenario} | - | DIFFERS |\n")
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(format!("{report}{line}").as_bytes());
        }
    }
    assert!(
        errors.is_empty(),
        "{scenario}: the daemons answer differently\n{errors}"
    );
}

/// Writes the bytes of a snapshot for `scripts/xterm-grid.mjs`, which replays them in xterm.js:
/// `<dir>/<scenario>.<daemon>.<cols>x<rows>.bin`, in NATAMUX_CONTRACT_DUMPS when set.
pub fn dump_snapshot(scenario: &str, daemon: &str, snapshot: &(u16, u16, Vec<u8>)) {
    let Ok(dir) = std::env::var("NATAMUX_CONTRACT_DUMPS") else {
        return;
    };
    let (cols, rows, bytes) = snapshot;
    let _ = std::fs::create_dir_all(&dir);
    let path = std::path::Path::new(&dir).join(format!("{scenario}.{daemon}.{cols}x{rows}.bin"));
    let _ = std::fs::write(path, bytes);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn shell_ids_and_tickets_become_placeholders() {
        let mut transcript = Transcript::new("node");
        transcript.note("hello", json!({ "type": "hello", "shellId": "abc" }));
        transcript.note("again", json!([{ "shellId": "xyz" }, { "shellId": "abc" }]));
        transcript.note("ticket", json!({ "ticket": "t" }));
        assert_eq!(transcript.steps[0].1["shellId"], "<shell 1>");
        assert_eq!(
            transcript.steps[1].1,
            json!([{ "shellId": "<shell 2>" }, { "shellId": "<shell 1>" }])
        );
        assert_eq!(transcript.steps[2].1["ticket"], "<ticket>");
    }

    #[test]
    fn equal_transcripts_pass_and_an_allowed_difference_too() {
        let (mut node, mut rust) = (Transcript::new("node"), Transcript::new("rust"));
        node.note("a", json!(1));
        rust.note("a", json!(1));
        node.note("b", json!("x"));
        rust.note("b", json!("y"));
        compare(
            "test",
            &node,
            &rust,
            &[Allowed {
                step: "b",
                reason: "known",
            }],
        );
    }

    #[test]
    #[should_panic(expected = "the daemons answer differently")]
    fn a_difference_that_is_not_allowed_fails() {
        let (mut node, mut rust) = (Transcript::new("node"), Transcript::new("rust"));
        node.note("a", json!(1));
        rust.note("a", json!(2));
        compare("test", &node, &rust, &[]);
    }
}
