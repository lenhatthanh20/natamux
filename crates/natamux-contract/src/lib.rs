//! A black-box contract suite: the same HTTP and WebSocket scenarios against the Node daemon and
//! the Rust daemon, and their answers compared (docs/rust-phase4.md, "Contract suite"). The
//! scenarios are the ignored tests of `tests/`; CI starts both daemons and runs them with
//! `--ignored`.

pub mod daemon;
pub mod events;
pub mod socket;
pub mod transcript;
