//! The shell of the terminal, apart from `launch` so `launch_ask` can use it without a cycle.

/// The shell of the terminal: PowerShell on Windows, a POSIX shell elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Posix,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Posix
        }
    }
}
