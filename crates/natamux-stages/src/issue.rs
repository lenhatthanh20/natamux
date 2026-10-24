//! Ports `packages/daemon/src/github/issue-ref.ts`.

use natamux_core::js::{is_js_space, js_trim};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueRef {
    pub number: u64,
    /// The repository of an issue address: "owner/name", for the closing line of a pull request.
    pub slug: Option<String>,
    /// The same for `gh --repo`, with the host when it is not github.com.
    pub repo: Option<String>,
}

fn number_of(digits: &str) -> u64 {
    digits.parse().unwrap_or(u64::MAX)
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// `^https?://([^/\s]+)/([^/\s]+)/([^/\s]+)/issues/(\d+)/?(?:[?#]\S*)?$`
fn parse_address(text: &str) -> Option<IssueRef> {
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))?;
    let mut parts = rest.splitn(4, '/');
    let (host, owner, name, tail) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let part_ok = |part: &str| !part.is_empty() && !part.chars().any(is_js_space);
    if !(part_ok(host) && part_ok(owner) && part_ok(name)) {
        return None;
    }
    let tail = tail.strip_prefix("issues/")?;
    let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
    let after = tail[digits..].strip_prefix('/').unwrap_or(&tail[digits..]);
    let query_ok =
        after.is_empty() || (after.starts_with(['?', '#']) && !after.chars().any(is_js_space));
    if digits == 0 || !query_ok {
        return None;
    }
    let slug = format!("{owner}/{name}");
    Some(IssueRef {
        number: number_of(&tail[..digits]),
        repo: Some(if host == "github.com" {
            slug.clone()
        } else {
            format!("{host}/{slug}")
        }),
        slug: Some(slug),
    })
}

/// `^(?:([\w.-]+\/[\w.-]+)#|#?)(\d+)$`
fn parse_key(text: &str) -> Option<IssueRef> {
    let slug_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    if let Some((slug, digits)) = text.rsplit_once('#')
        && all_digits(digits)
        && let Some((owner, name)) = slug.split_once('/')
        && !owner.is_empty()
        && !name.is_empty()
        && owner.chars().chain(name.chars()).all(slug_char)
    {
        return Some(IssueRef {
            number: number_of(digits),
            slug: Some(slug.to_string()),
            repo: Some(slug.to_string()),
        });
    }
    let digits = text.strip_prefix('#').unwrap_or(text);
    all_digits(digits).then(|| IssueRef {
        number: number_of(digits),
        slug: None,
        repo: None,
    })
}

/// An issue in "#42", "42", "owner/name#42" or an issue address (a comment link too).
pub fn parse_issue_ref(text: &str) -> Option<IssueRef> {
    let text = js_trim(text);
    parse_address(text).or_else(|| parse_key(text))
}

/// The issue number in "#42", "42" or an issue address.
pub fn issue_number(text: &str) -> Option<u64> {
    parse_issue_ref(text).map(|issue| issue.number)
}

/// The fields of `gh issue view --json ...` that the context uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GhIssue {
    pub number: Option<u64>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub labels: Vec<Option<String>>,
    pub comments: Vec<GhComment>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GhComment {
    pub author: Option<String>,
    pub body: Option<String>,
    pub created_at: Option<String>,
}

fn text_at(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

impl GhIssue {
    /// Reads the JSON of gh; a field of another type counts as missing.
    pub fn from_json(text: &str) -> Option<GhIssue> {
        let value: Value = serde_json::from_str(text).ok()?;
        let list = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        Some(GhIssue {
            number: value.get("number").and_then(Value::as_u64),
            title: text_at(&value, "title"),
            body: text_at(&value, "body"),
            url: text_at(&value, "url"),
            labels: list("labels")
                .iter()
                .map(|label| text_at(label, "name"))
                .collect(),
            comments: list("comments")
                .iter()
                .map(|comment| GhComment {
                    author: comment
                        .get("author")
                        .and_then(|author| text_at(author, "login")),
                    body: text_at(comment, "body"),
                    created_at: text_at(comment, "createdAt"),
                })
                .collect(),
        })
    }
}

/// The issue as markdown, for the context of a task.
pub fn issue_context(issue: &GhIssue) -> String {
    let labels: Vec<&str> = issue
        .labels
        .iter()
        .filter_map(|label| label.as_deref().filter(|name| !name.is_empty()))
        .collect();
    let comments: Vec<String> = issue
        .comments
        .iter()
        .map(|comment| {
            let date = comment
                .created_at
                .as_deref()
                .filter(|date| !date.is_empty())
                .map_or(String::new(), |date| format!(", {date}"));
            [
                format!(
                    "### {}{date}",
                    comment.author.as_deref().unwrap_or("someone")
                ),
                String::new(),
                js_trim(comment.body.as_deref().unwrap_or_default()).to_string(),
            ]
            .join("\n")
        })
        .collect();
    let title = issue.title.clone().unwrap_or_else(|| {
        format!(
            "Issue #{}",
            issue.number.map(|n| n.to_string()).unwrap_or_default()
        )
    });
    let body = js_trim(issue.body.as_deref().unwrap_or_default());
    let mut lines = vec![
        format!("# {title}"),
        String::new(),
        format!("Source: {}", issue.url.as_deref().unwrap_or_default()),
    ];
    if !labels.is_empty() {
        lines.push(format!("Labels: {}", labels.join(", ")));
    }
    lines.push(String::new());
    lines.push(if body.is_empty() {
        "(No description.)".into()
    } else {
        body.to_string()
    });
    if !comments.is_empty() {
        lines.extend([
            "".into(),
            "## Comments".into(),
            "".into(),
            comments.join("\n\n"),
        ]);
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
#[path = "issue_tests.rs"]
mod tests;
