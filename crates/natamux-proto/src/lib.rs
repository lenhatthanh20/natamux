//! Wire types of the Natamux daemon's HTTP and WebSocket API (L0). Names and values match the
//! TypeScript schemas in `packages/core`, which stay the source of truth until phase 4.

pub mod agents {
    pub mod claude_code;
}
pub mod events {
    pub mod kind;
    pub mod task_event;
}
pub mod projects {
    pub mod repo;
}
pub mod tasks {
    pub mod attachments;
    pub mod task;
}
pub mod terminals {
    pub mod messages;
    pub mod query;
}
