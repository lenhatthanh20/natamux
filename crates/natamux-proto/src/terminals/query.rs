//! The query of `GET /tasks/:id/terminal`, the terminal socket.

/// What a client tells the daemon when it connects. `visible` is false when the terminal is not
/// on screen: only a visible one takes the shell's size. `shell_id` resumes that shell, from
/// `offset` when the daemon's ring still has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalQuery {
    pub ticket: String,
    pub cols: u16,
    pub rows: u16,
    pub terminal: String,
    pub visible: bool,
    pub shell_id: Option<String>,
    pub offset: Option<u64>,
}

impl TerminalQuery {
    pub fn to_query_string(&self) -> String {
        let mut pairs = vec![
            ("ticket", self.ticket.clone()),
            ("cols", self.cols.to_string()),
            ("rows", self.rows.to_string()),
            ("terminal", self.terminal.clone()),
            ("visible", if self.visible { "1" } else { "0" }.to_string()),
        ];
        if let Some(shell_id) = &self.shell_id {
            pairs.push(("shellId", shell_id.clone()));
            if let Some(offset) = self.offset {
                pairs.push(("offset", offset.to_string()));
            }
        }
        pairs
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// Percent-encodes everything but the unreserved characters of RFC 3986.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> TerminalQuery {
        TerminalQuery {
            ticket: "a-b_c".into(),
            cols: 80,
            rows: 24,
            terminal: "1".into(),
            visible: true,
            shell_id: None,
            offset: None,
        }
    }

    #[test]
    fn a_first_connect_asks_for_the_shell_as_it_is() {
        assert_eq!(
            query().to_query_string(),
            "ticket=a-b_c&cols=80&rows=24&terminal=1&visible=1"
        );
    }

    #[test]
    fn a_resume_sends_the_shell_and_the_offset_when_known() {
        let mut resume = query();
        resume.visible = false;
        resume.shell_id = Some("s1".into());
        assert_eq!(
            resume.to_query_string(),
            "ticket=a-b_c&cols=80&rows=24&terminal=1&visible=0&shellId=s1"
        );
        resume.offset = Some(4096);
        assert!(
            resume
                .to_query_string()
                .ends_with("&shellId=s1&offset=4096")
        );
    }

    #[test]
    fn an_offset_without_a_shell_is_not_sent() {
        let mut odd = query();
        odd.offset = Some(10);
        assert!(!odd.to_query_string().contains("offset"));
    }

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode("a b/c+d=é"), "a%20b%2Fc%2Bd%3D%C3%A9");
    }
}
