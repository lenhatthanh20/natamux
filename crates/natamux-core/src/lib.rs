//! Task rules of the Rust daemon (L0): the zod schemas with their texts, and the pure task logic of
//! `packages/core` that `natamux-proto` does not have. Types come from `natamux-proto`.

pub mod agents {
    pub mod custom_agents;
    pub mod hook_settings;
}
pub mod formats;
pub mod git {
    pub mod commit_message;
    pub mod review_file;
    pub mod review_pin;
}
pub mod js;
pub mod json;
pub mod zod;
pub mod events {
    pub mod apply;
    pub mod derive;
    pub mod moves;
    pub mod reopen;
    pub mod schema;
    #[cfg(test)]
    pub(crate) mod test_fixture;
}
pub mod stages {
    pub mod capabilities;
    pub mod lanes;
    pub mod position;
    pub mod terminal_lane;
}
pub mod workspaces {
    pub mod branch_name;
    pub mod private_dir;
    pub mod safe_write;
    pub mod slug;
}
pub mod tasks {
    pub mod attachments;
    pub mod dependencies;
    pub mod inputs;
    pub mod state;
    pub mod task;
    mod task_parts;
}
