use std::fmt;

/// Every failure the app can show. Texts follow the `natamux` command's (packages/cli/src/client.ts).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    NoToken,
    BadUrl(String),
    Unreachable {
        url: String,
        reason: String,
    },
    Unauthorized,
    /// Any other answer that is not 2xx, with the daemon's message when it gave one.
    Status {
        status: u16,
        message: String,
    },
    /// An answer the client cannot read.
    BadAnswer(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::NoToken => write!(
                f,
                "No token for the daemon. Set NATAMUX_TOKEN, or start the Natamux app once so it saves one."
            ),
            ClientError::BadUrl(url) => {
                write!(f, "NATAMUX_URL must start with http:// (got {url:?}).")
            }
            ClientError::Unreachable { url, reason } => {
                write!(f, "Cannot reach the daemon at {url}: {reason}")
            }
            ClientError::Unauthorized => {
                write!(f, "The daemon rejected the token. Check NATAMUX_TOKEN.")
            }
            ClientError::Status { status, message } => {
                write!(f, "The daemon answered {status}: {message}")
            }
            ClientError::BadAnswer(reason) => {
                write!(f, "The daemon sent an answer the app cannot read: {reason}")
            }
        }
    }
}

impl std::error::Error for ClientError {}
