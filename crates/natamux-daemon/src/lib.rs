//! The Rust daemon (L2): the HTTP and WebSocket server, auth, and the terminals (phase 4, part 1
//! of `docs/plan-rust-migration.md`), the tasks (part 2), the stage engine with agent start
//! (part 3), the hooks, signals, activity and usage of the agents (part 4), and the workspaces, git
//! and GitHub (part 5). It answers as the Node daemon in `packages/daemon` does.

pub mod agents {
    pub mod errors;
    pub mod generic_signals;
    mod hook_activity;
    pub mod hook_service;
    mod hook_usage;
    pub mod idle_pause;
    pub mod runner;
    pub mod task_access;
    mod usage_file;
    pub mod usage_service;
}
pub mod config;
pub mod github {
    pub mod issue_importer;
    pub mod issue_linker;
    pub mod pull_request_watcher;
}
pub mod http {
    pub mod agent_rules;
    pub mod auth;
    pub mod body;
    pub mod edge_rules;
    pub mod errors;
    pub mod event_socket;
    pub mod health;
    pub mod routes {
        pub mod agents;
        pub mod github;
        pub mod projects;
        pub mod review_comments;
        pub mod stages;
        pub mod tasks;
        pub mod terminals;
        pub mod workspace_files;
        pub mod workspaces;
    }
    pub mod server;
    pub mod terminal_socket;
    pub mod web;
    #[cfg(test)]
    mod web_tests;
}
pub mod projects {
    pub mod errors;
    pub mod repo_add;
    pub mod repo_store;
    pub mod repo_trust;
    pub mod repos;
    pub mod settings;
    pub mod settings_parse;
    pub mod settings_trim;
}
pub mod shared {
    pub mod event_bus;
    pub mod jobs;
    pub mod map_limit;
    pub mod scheduler;
    pub mod slug;
    pub mod ttl_cache;
    pub mod turns;
}
pub mod terminals {
    pub mod input_trace;
    pub mod limits;
    pub mod login_path;
    pub mod open_shells;
    mod open_shells_pause;
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
pub mod stages {
    pub mod act;
    pub mod context;
    pub mod errors;
    mod gate;
    mod gate_ci;
    pub mod launcher;
    mod launcher_resume;
    pub mod run;
    pub mod runner;
    pub mod shells;
    mod shells_finish;
    pub mod stage_tickets;
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
pub mod workspaces {
    pub mod agent_once;
    pub mod branch_name;
    pub mod branch_sync;
    pub mod changes;
    pub mod create;
    pub mod diff_stats;
    pub mod dirty;
    pub mod errors;
    pub mod fan_out;
    pub mod files;
    pub mod new_workspace;
    pub mod publish;
    pub mod remove;
    pub mod review_comments;
    pub mod service;
    pub mod setup_script;
}
