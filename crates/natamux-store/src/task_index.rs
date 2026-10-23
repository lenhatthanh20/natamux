//! The index (`task-index.ts`): a cache of the task files plus the event logs, in
//! `<data>/index.sqlite`. The task files are the source of truth. Same tables and statements as
//! the Node daemon, so either daemon can read the index the other wrote.

use std::collections::HashMap;
use std::path::Path;

use natamux_core::events::schema::validate_task_event;
use natamux_core::json::{parse_js, to_js_string, to_js_string_of};
use natamux_core::tasks::task::validate_task;
use natamux_proto::events::task_event::{StoredEvent, TaskEvent};
use natamux_proto::tasks::task::Task;
use rusqlite::{Connection, params};
use serde_json::Value;

use crate::event_log::LoggedEvent;
use crate::task_files::FileStat;

// task_files came later. An older index gets it empty, so the next start reads every file again.
const SCHEMA: &str = "
create table if not exists tasks_index (
  id text primary key,
  title text not null,
  state text not null,
  priority text not null,
  updated_at text not null,
  data text not null
);
create table if not exists events (
  seq integer primary key autoincrement,
  task_id text not null,
  source text not null,
  type text not null,
  at text not null,
  payload text not null,
  accepted integer not null
);
create index if not exists events_by_task on events (task_id, seq);
create table if not exists task_files (
  task_id text not null,
  kind text not null,
  mtime real not null,
  size integer not null,
  primary key (task_id, kind)
);
";

const UPSERT_TASK: &str = "
insert into tasks_index (id, title, state, priority, updated_at, data)
values (?1, ?2, ?3, ?4, ?5, ?6)
on conflict (id) do update set
  title = excluded.title,
  state = excluded.state,
  priority = excluded.priority,
  updated_at = excluded.updated_at,
  data = excluded.data
";

const UPSERT_FILE: &str = "
insert into task_files (task_id, kind, mtime, size) values (?, ?, ?, ?)
on conflict (task_id, kind) do update set mtime = excluded.mtime, size = excluded.size
";

const APPEND_EVENT: &str =
    "insert into events (task_id, source, type, at, payload, accepted) values (?, ?, ?, ?, ?, ?)";

/// The task file and the event log of a task, as they were when the index last matched them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TaskFiles {
    pub task: Option<FileStat>,
    pub log: Option<FileStat>,
}

/// One change of a task. A part that is left out stays as it is.
#[derive(Clone, Debug, Default)]
pub struct TaskChange<'a> {
    pub task: Option<&'a Task>,
    pub events: &'a [LoggedEvent],
    pub files: TaskFiles,
}

pub type IndexResult<T> = Result<T, String>;

fn sql(error: rusqlite::Error) -> String {
    error.to_string()
}

pub struct TaskIndex {
    db: Connection,
}

impl TaskIndex {
    pub fn open(path: &Path) -> IndexResult<TaskIndex> {
        let db = Connection::open(path).map_err(sql)?;
        db.pragma_update(None, "journal_mode", "wal").map_err(sql)?;
        // Safe with WAL. A crash loses at most the last commits, and init() reads those files again.
        db.pragma_update(None, "synchronous", "normal")
            .map_err(sql)?;
        db.execute_batch(SCHEMA).map_err(sql)?;
        Ok(TaskIndex { db })
    }

    /// The connection, for tests that look at the pragmas.
    pub fn connection(&self) -> &Connection {
        &self.db
    }

    pub fn delete_task(&mut self, task_id: &str) -> IndexResult<()> {
        self.transaction(|db| {
            db.prepare_cached("delete from tasks_index where id = ?")?
                .execute([task_id])?;
            db.prepare_cached("delete from events where task_id = ?")?
                .execute([task_id])?;
            db.prepare_cached("delete from task_files where task_id = ?")?
                .execute([task_id])?;
            Ok(())
        })
    }

    pub fn upsert_task(&mut self, task: &Task) -> IndexResult<()> {
        upsert_task(&self.db, task).map(drop).map_err(sql)
    }

    /// One SQLite transaction per change, so a change costs one commit.
    pub fn record(&mut self, task_id: &str, change: &TaskChange) -> IndexResult<()> {
        self.transaction(|db| {
            if let Some(task) = change.task {
                upsert_task(db, task)?;
            }
            for logged in change.events {
                append_event(db, &logged.event, logged.accepted)?;
            }
            store_files(db, task_id, change.files)
        })
    }

    pub fn replace_all_tasks(
        &mut self,
        tasks: &[&Task],
        files: &[(String, TaskFiles)],
    ) -> IndexResult<()> {
        self.transaction(|db| {
            db.execute_batch("delete from tasks_index; delete from task_files;")?;
            for task in tasks {
                upsert_task(db, task)?;
            }
            for (task_id, stats) in files {
                store_files(db, task_id, *stats)?;
            }
            Ok(())
        })
    }

    /// Parsed with the schema: a row written by an older version may miss newer fields.
    pub fn list_tasks(&self, state: Option<&str>) -> IndexResult<Vec<Task>> {
        let rows: Vec<String> = match state {
            None => {
                let mut statement = self
                    .db
                    .prepare_cached("select data from tasks_index order by updated_at desc, id")
                    .map_err(sql)?;
                statement
                    .query_map([], |row| row.get(0))
                    .map_err(sql)?
                    .collect::<Result<_, _>>()
                    .map_err(sql)?
            }
            Some(state) => {
                let mut statement = self
                    .db
                    .prepare_cached(
                        "select data from tasks_index where state = ? order by updated_at desc, id",
                    )
                    .map_err(sql)?;
                statement
                    .query_map([state], |row| row.get(0))
                    .map_err(sql)?
                    .collect::<Result<_, _>>()
                    .map_err(sql)?
            }
        };
        rows.iter()
            .map(|data| {
                let value = parse_js(data).map_err(|error| error.to_string())?;
                validate_task(&value).map_err(|error| error.to_json_text())
            })
            .collect()
    }

    pub fn task_files(&self) -> IndexResult<HashMap<String, TaskFiles>> {
        let mut statement = self
            .db
            .prepare_cached("select task_id, kind, mtime, size from task_files")
            .map_err(sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(sql)?;
        let mut files: HashMap<String, TaskFiles> = HashMap::new();
        for row in rows {
            let (task_id, kind, mtime_ms, size) = row.map_err(sql)?;
            let stat = FileStat {
                mtime_ms,
                size: size.max(0) as u64,
            };
            let entry = files.entry(task_id).or_default();
            match kind.as_str() {
                "task" => entry.task = Some(stat),
                "log" => entry.log = Some(stat),
                _ => {}
            }
        }
        Ok(files)
    }

    pub fn append_event(&mut self, event: &TaskEvent, accepted: bool) -> IndexResult<()> {
        append_event(&self.db, event, accepted)
            .map(drop)
            .map_err(sql)
    }

    pub fn count_events(&self, task_id: &str) -> IndexResult<u64> {
        self.db
            .prepare_cached("select count(*) as count from events where task_id = ?")
            .and_then(|mut statement| statement.query_row([task_id], |row| row.get::<_, i64>(0)))
            .map(|count| count.max(0) as u64)
            .map_err(sql)
    }

    /// Puts the events of a task back from its event log, in order.
    pub fn replace_events(&mut self, task_id: &str, events: &[LoggedEvent]) -> IndexResult<()> {
        self.transaction(|db| {
            db.prepare_cached("delete from events where task_id = ?")?
                .execute([task_id])?;
            for logged in events {
                append_event(db, &logged.event, logged.accepted)?;
            }
            Ok(())
        })
    }

    pub fn list_events(&self, task_id: &str) -> IndexResult<Vec<StoredEvent>> {
        let mut statement = self
            .db
            .prepare_cached("select * from events where task_id = ? order by seq")
            .map_err(sql)?;
        let rows = statement
            .query_map([task_id], |row| {
                Ok((
                    row.get::<_, i64>("seq")?,
                    row.get::<_, String>("task_id")?,
                    row.get::<_, String>("source")?,
                    row.get::<_, String>("type")?,
                    row.get::<_, String>("at")?,
                    row.get::<_, String>("payload")?,
                    row.get::<_, i64>("accepted")?,
                ))
            })
            .map_err(sql)?;
        let mut events = Vec::new();
        for row in rows {
            let (seq, task_id, source, kind, at, payload, accepted) = row.map_err(sql)?;
            let payload = parse_js(&payload).map_err(|error| error.to_string())?;
            let value = serde_json::json!({
                "taskId": task_id, "source": source, "type": kind, "at": at, "payload": payload,
            });
            let event = validate_task_event(&value).map_err(|error| error.to_json_text())?;
            events.push(StoredEvent {
                seq: seq.max(0) as u64,
                accepted: accepted == 1,
                event,
            });
        }
        Ok(events)
    }

    fn transaction(
        &mut self,
        work: impl FnOnce(&Connection) -> rusqlite::Result<()>,
    ) -> IndexResult<()> {
        let transaction = self.db.transaction().map_err(sql)?;
        work(&transaction).map_err(sql)?;
        transaction.commit().map_err(sql)
    }
}

fn upsert_task(db: &Connection, task: &Task) -> rusqlite::Result<usize> {
    db.prepare_cached(UPSERT_TASK)?.execute(params![
        task.id,
        task.title,
        task.state.name(),
        task.priority,
        task.updated_at,
        to_js_string_of(task),
    ])
}

fn append_event(db: &Connection, event: &TaskEvent, accepted: bool) -> rusqlite::Result<usize> {
    let payload = to_js_string(&Value::Object(event.payload.clone()));
    db.prepare_cached(APPEND_EVENT)?.execute(params![
        event.task_id,
        event.source,
        event.kind,
        event.at,
        payload,
        i64::from(accepted),
    ])
}

fn store_files(db: &Connection, task_id: &str, files: TaskFiles) -> rusqlite::Result<()> {
    for (kind, stat) in [("task", files.task), ("log", files.log)] {
        if let Some(stat) = stat {
            db.prepare_cached(UPSERT_FILE)?.execute(params![
                task_id,
                kind,
                stat.mtime_ms,
                stat.size as i64
            ])?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "task_index_tests.rs"]
mod tests;
