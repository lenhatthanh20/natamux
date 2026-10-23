//! The Rust daemon (L2): the HTTP and WebSocket server, auth, and the terminals (phase 4, part 1
//! of `docs/plan-rust-migration.md`) and the tasks (part 2). It answers as the Node daemon in
//! `packages/daemon` does.

pub mod config;
pub mod http {
    pub mod auth;
    pub mod body;
    pub mod errors;
    pub mod event_socket;
    pub mod routes {
        pub mod tasks;
        pub mod terminals;
    }
    pub mod server;
    pub mod terminal_socket;
}
pub mod projects {
    pub mod repos;
}
pub mod shared {
    pub mod event_bus;
    pub mod jobs;
    pub mod scheduler;
    pub mod slug;
    pub mod ttl_cache;
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
pub mod tasks {
    pub mod errors;
    pub mod helpers;
    pub mod records;
    mod removal;
    pub mod service;
    pub mod store;
    pub mod templates;
    pub mod upload;
}
