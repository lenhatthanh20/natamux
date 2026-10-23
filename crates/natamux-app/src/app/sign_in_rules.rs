//! The Sign in screen: its texts, what Connect sends, and what an answer says (`SignIn.tsx`,
//! `App.tsx`).

pub const REJECTED: &str =
    "The daemon rejected that token. Paste the token it printed when it started.";
pub const BLANK_REASON: &str = "Paste the token first";
pub const INTRO: &str =
    "Paste the token the daemon printed when it started. It stays on this computer.";
pub const HELP: &str = "It is NATAMUX_TOKEN of the daemon. Without it, the daemon makes one and prints it when it starts.";

/// The line under the field after a failed connect: a 401 means the token is wrong; any other
/// error says what the client said.
pub fn connect_problem(status: Option<u16>, message: &str) -> String {
    if status == Some(401) {
        REJECTED.to_owned()
    } else {
        message.to_owned()
    }
}

/// The token to send: the field trimmed, None when it is blank.
pub fn token_to_send(field: &str) -> Option<String> {
    let token = field.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goes_back_to_the_token_form_when_the_daemon_rejects_the_token() {
        assert_eq!(connect_problem(Some(401), "Unauthorized"), REJECTED);
    }

    #[test]
    fn says_what_the_client_said_for_other_errors() {
        let text = "Cannot reach the daemon. Is it running?";
        assert_eq!(connect_problem(None, text), text);
        assert_eq!(connect_problem(Some(500), "boom"), "boom");
    }

    #[test]
    fn sends_the_trimmed_token_and_nothing_for_a_blank_field() {
        assert_eq!(token_to_send("  abc \n"), Some("abc".to_owned()));
        assert_eq!(token_to_send("   "), None);
        assert_eq!(token_to_send(""), None);
    }
}
