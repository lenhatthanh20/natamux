//! Task rules of the Rust daemon (L0): the zod schemas with their texts, and the pure task logic of
//! `packages/core` that `natamux-proto` does not have. Types come from `natamux-proto`.

pub mod formats;
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
    pub mod position;
    pub mod terminal_lane;
}
pub mod tasks {
    pub mod attachments;
    pub mod dependencies;
    pub mod inputs;
    pub mod state;
    pub mod task;
    mod task_parts;
}
