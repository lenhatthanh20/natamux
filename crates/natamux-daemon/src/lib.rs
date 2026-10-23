//! The Rust daemon (L2): the HTTP and WebSocket server, auth, and the terminals (phase 4, part 1
//! of `docs/plan-rust-migration.md`). It answers as the Node daemon in `packages/daemon` does.

pub mod config;
pub mod http {
    pub mod auth;
    pub mod errors;
    pub mod routes {
        pub mod terminals;
    }
    pub mod server;
    pub mod terminal_socket;
}
pub mod shared {
    pub mod event_bus;
    pub mod scheduler;
}
pub mod terminals {
    pub mod limits;
    pub mod login_path;
    pub mod open_shells;
    mod open_shells_sync;
    pub mod service;
    pub mod session_input;
    pub mod session_lifecycle;
    pub mod session_output;
    pub mod session_snapshot;
    pub mod shell_files;
    pub mod shells;
    pub mod tickets;
    pub mod types;
}
pub mod start;
