//! Task files and the index of the Rust daemon (L1): Markdown with YAML front matter as
//! gray-matter and js-yaml 3 write it, the JSONL event logs, the SQLite index, atomic writes and
//! the file sync of `task-sync.ts`.

pub mod attachments;
pub mod event_log;
pub mod front_matter;
pub mod task_files;
pub mod task_index;
pub mod task_sync;
pub mod watch;
pub mod yaml_dump;
pub mod yaml_load;
pub mod yaml_types;
