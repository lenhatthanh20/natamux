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

impl TerminalQuery {
    /// The daemon's side: `terminalQuerySchema` in `packages/daemon/src/http/routes/terminals.ts`.
    /// None when a value breaks a rule or a name comes twice (Node's query parser makes that a
    /// list, which the schema refuses). Names it does not know are left out.
    pub fn parse(query: &str) -> Option<TerminalQuery> {
        let mut values: Vec<(String, String)> = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (name, value) = (decode(name), decode(value));
            if values.iter().any(|(known, _)| *known == name) {
                return None;
            }
            values.push((name, value));
        }
        let get = |name: &str| {
            values
                .iter()
                .find(|(known, _)| known == name)
                .map(|(_, value)| value.as_str())
        };
        let ticket = get("ticket").filter(|ticket| (1..=256).contains(&utf16_len(ticket)))?;
        let terminal = get("terminal").unwrap_or(super::messages::MAIN_TERMINAL);
        let visible = match get("visible") {
            None | Some("1") => true,
            Some("0") => false,
            Some(_) => return None,
        };
        let shell_id = match get("shellId") {
            Some(id) if (1..=64).contains(&utf16_len(id)) => Some(id.to_string()),
            Some(_) => return None,
            None => None,
        };
        let offset = match get("offset") {
            Some(text) => Some(js_integer(text)?),
            None => None,
        };
        let size = |name| {
            js_integer(get(name)?)
                .filter(|value| (1..=u64::from(super::messages::MAX_SIZE)).contains(value))
                .and_then(|value| u16::try_from(value).ok())
        };
        Some(TerminalQuery {
            ticket: ticket.to_string(),
            cols: size("cols")?,
            rows: size("rows")?,
            terminal: is_terminal_name(terminal).then(|| terminal.to_string())?,
            visible,
            shell_id,
            offset,
        })
    }
}

/// `^[1-9]\d{0,2}$`: a terminal of a task is named by a number from 1 to 999.
pub fn is_terminal_name(name: &str) -> bool {
    (1..=3).contains(&name.len())
        && name.bytes().all(|byte| byte.is_ascii_digit())
        && !name.starts_with('0')
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `z.coerce.number().int().min(0)`: what JavaScript's `Number()` reads as a whole number that is
/// not negative. Blank text is 0. Hex and binary forms are not read (a listed difference).
fn js_integer(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0);
    }
    let number: f64 = text.parse().ok()?;
    let whole = number.is_finite() && number >= 0.0 && number.fract() == 0.0;
    // Above 2^53 a JavaScript number is no longer exact; such offsets never occur.
    (whole && number <= 9_007_199_254_740_991.0).then_some(number as u64)
}

/// Percent-decoding as Node's `querystring`: `+` is a space, and a broken escape stays as it is.
pub fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|byte| (*byte as char).to_digit(16));
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' => match (hex(index + 1), hex(index + 2)) {
                (Some(high), Some(low)) => {
                    out.push((high * 16 + low) as u8);
                    index += 2;
                }
                _ => out.push(b'%'),
            },
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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

    fn parsed(text: &str) -> Option<TerminalQuery> {
        TerminalQuery::parse(text)
    }

    #[test]
    fn the_daemon_reads_what_the_client_writes() {
        let mut resume = query();
        resume.visible = false;
        resume.shell_id = Some("s 1/é".into());
        resume.offset = Some(4096);
        assert_eq!(parsed(&resume.to_query_string()), Some(resume));
        assert_eq!(
            parsed("ticket=t&cols=80&rows=24"),
            Some(TerminalQuery {
                ticket: "t".into(),
                ..query()
            })
        );
    }

    #[test]
    fn the_daemon_refuses_what_the_schema_refuses() {
        for bad in [
            "cols=80&rows=24",
            "ticket=&cols=80&rows=24",
            "ticket=t&cols=0&rows=24",
            "ticket=t&cols=1001&rows=24",
            "ticket=t&cols=8.5&rows=24",
            "ticket=t&cols=80",
            "ticket=t&cols=80&rows=24&terminal=0",
            "ticket=t&cols=80&rows=24&terminal=1000",
            "ticket=t&cols=80&rows=24&visible=yes",
            "ticket=t&cols=80&rows=24&offset=-1",
            "ticket=t&ticket=u&cols=80&rows=24",
            "ticket=t&cols=80&rows=24&shellId=",
        ] {
            assert_eq!(parsed(bad), None, "{bad}");
        }
        let long = format!("ticket={}&cols=80&rows=24", "a".repeat(257));
        assert_eq!(parsed(&long), None);
    }

    #[test]
    fn numbers_are_read_as_javascript_reads_them() {
        let read = parsed("ticket=t&cols=%2080%20&rows=1e2&offset=").unwrap();
        assert_eq!((read.cols, read.rows, read.offset), (80, 100, Some(0)));
        assert_eq!(
            parsed("ticket=t&cols=80&rows=24&other=x&terminal=12")
                .unwrap()
                .terminal,
            "12"
        );
    }

    #[test]
    fn decodes_as_node_querystring() {
        assert_eq!(decode("a+b%2Fc%zz%C3%A9"), "a b/c%zzé");
    }

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode("a b/c+d=é"), "a%20b%2Fc%2Bd%3D%C3%A9");
    }
}
