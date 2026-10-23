//! Wire types of the Natamux daemon's HTTP and WebSocket API (L0). Names and values match the
//! TypeScript schemas in `packages/core`, which stay the source of truth until phase 4.

pub mod agents {
    pub mod activity;
    pub mod claude_code;
    pub mod start;
}
pub mod events {
    pub mod kind;
    pub mod task_event;
}
pub mod projects {
    pub mod issues;
    pub mod repo;
    pub mod settings_rules;
}
pub mod stages {
    pub mod actions;
    pub mod capabilities;
    pub mod catalog;
    mod catalog_text;
    mod field_rules;
    pub mod pipeline_rules;
    pub mod position;
    pub mod progress;
    pub mod prompt;
    pub mod resolve;
    pub mod schema;
    pub mod terminal_lane;
}
pub mod tasks {
    pub mod attachments;
    pub mod job;
    pub mod task;
    pub mod template;
}
pub mod terminals {
    pub mod ended;
    pub mod list;
    pub mod messages;
    pub mod query;
}
pub mod workspaces {
    pub mod changes;
    pub mod files;
    pub mod workspace;
}
