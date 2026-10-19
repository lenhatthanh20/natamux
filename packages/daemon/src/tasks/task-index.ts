import { DatabaseSync } from 'node:sqlite'
import {
  type Task,
  type TaskEvent,
  type TaskState,
  taskEventSchema,
  taskSchema,
} from '@natamux/core'
import type { FileStat } from './task-files.js'

// task_files came later. An older index gets it empty, so the next start reads every file again.
const SCHEMA = `
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
`

const UPSERT_TASK = `
insert into tasks_index (id, title, state, priority, updated_at, data)
values (@id, @title, @state, @priority, @updatedAt, @data)
on conflict (id) do update set
  title = excluded.title,
  state = excluded.state,
  priority = excluded.priority,
  updated_at = excluded.updated_at,
  data = excluded.data
`

const UPSERT_FILE = `
insert into task_files (task_id, kind, mtime, size) values (?, ?, ?, ?)
on conflict (task_id, kind) do update set mtime = excluded.mtime, size = excluded.size
`

export type StoredEvent = {
  seq: number
  accepted: boolean
  event: TaskEvent
}

export type IndexedEvent = { event: TaskEvent; accepted: boolean }

// The task file (`task`) and the event log (`log`) of a task, as they were when the index last
// matched them.
export type TaskFiles = { task?: FileStat; log?: FileStat }

// One change of a task. A part that is left out stays as it is.
export type TaskChange = {
  task?: Task
  events?: readonly IndexedEvent[]
  files?: TaskFiles
}

type TaskRow = { data: string }

type FileRow = { task_id: string; kind: keyof TaskFiles; mtime: number; size: number }

type EventRow = {
  seq: number
  task_id: string
  source: string
  type: string
  at: string
  payload: string
  accepted: number
}

function prepareStatements(db: DatabaseSync) {
  return {
    upsertTask: db.prepare(UPSERT_TASK),
    appendEvent: db.prepare(
      'insert into events (task_id, source, type, at, payload, accepted) values (?, ?, ?, ?, ?, ?)',
    ),
    deleteTask: db.prepare('delete from tasks_index where id = ?'),
    deleteEvents: db.prepare('delete from events where task_id = ?'),
    deleteFiles: db.prepare('delete from task_files where task_id = ?'),
    upsertFile: db.prepare(UPSERT_FILE),
    countEvents: db.prepare('select count(*) as count from events where task_id = ?'),
    listEvents: db.prepare('select * from events where task_id = ? order by seq'),
    listTasks: db.prepare('select data from tasks_index order by updated_at desc, id'),
    listTasksIn: db.prepare(
      'select data from tasks_index where state = ? order by updated_at desc, id',
    ),
    listFiles: db.prepare('select task_id, kind, mtime, size from task_files'),
  }
}

// The task files are the source of truth. This index is a cache of them plus the event log.
export class TaskIndex {
  private readonly db: DatabaseSync
  // Prepared once: the hot path runs them for every hook.
  private readonly statements: ReturnType<typeof prepareStatements>

  constructor(path: string) {
    this.db = new DatabaseSync(path)
    this.db.exec('pragma journal_mode = wal')
    // Safe with WAL. A crash loses at most the last commits, and init() reads those files again.
    this.db.exec('pragma synchronous = normal')
    this.db.exec(SCHEMA)
    this.statements = prepareStatements(this.db)
  }

  close(): void {
    this.db.close()
  }

  deleteTask(taskId: string): void {
    this.transaction(() => {
      this.statements.deleteTask.run(taskId)
      this.statements.deleteEvents.run(taskId)
      this.statements.deleteFiles.run(taskId)
    })
  }

  upsertTask(task: Task): void {
    this.statements.upsertTask.run({
      id: task.id,
      title: task.title,
      state: task.state,
      priority: task.priority,
      updatedAt: task.updatedAt,
      data: JSON.stringify(task),
    })
  }

  // One SQLite transaction per change, so a change costs one commit.
  record(taskId: string, change: TaskChange): void {
    this.transaction(() => {
      if (change.task !== undefined) {
        this.upsertTask(change.task)
      }
      for (const { event, accepted } of change.events ?? []) {
        this.appendEvent(event, accepted)
      }
      this.storeFiles(taskId, change.files ?? {})
    })
  }

  replaceAllTasks(tasks: readonly Task[], files: ReadonlyMap<string, TaskFiles> = new Map()): void {
    this.transaction(() => {
      this.db.exec('delete from tasks_index')
      this.db.exec('delete from task_files')
      for (const task of tasks) {
        this.upsertTask(task)
      }
      for (const [taskId, stats] of files) {
        this.storeFiles(taskId, stats)
      }
    })
  }

  // Parsed with the schema: a row written by an older version may miss newer fields.
  listTasks(state?: TaskState): Task[] {
    const { listTasks, listTasksIn } = this.statements
    const rows = state === undefined ? listTasks.all() : listTasksIn.all(state)
    return (rows as TaskRow[]).map((row) => taskSchema.parse(JSON.parse(row.data)))
  }

  taskFiles(): Map<string, TaskFiles> {
    const files = new Map<string, TaskFiles>()
    for (const row of this.statements.listFiles.all() as FileRow[]) {
      const entry = files.get(row.task_id) ?? {}
      entry[row.kind] = { mtimeMs: row.mtime, size: row.size }
      files.set(row.task_id, entry)
    }
    return files
  }

  appendEvent(event: TaskEvent, accepted: boolean): void {
    this.statements.appendEvent.run(
      event.taskId,
      event.source,
      event.type,
      event.at,
      JSON.stringify(event.payload),
      accepted ? 1 : 0,
    )
  }

  countEvents(taskId: string): number {
    const row = this.statements.countEvents.get(taskId) as { count: number }
    return row.count
  }

  // Puts the events of a task back from its event log, in order.
  replaceEvents(taskId: string, events: readonly IndexedEvent[]): void {
    this.transaction(() => {
      this.statements.deleteEvents.run(taskId)
      for (const { event, accepted } of events) {
        this.appendEvent(event, accepted)
      }
    })
  }

  listEvents(taskId: string): StoredEvent[] {
    const rows = this.statements.listEvents.all(taskId) as EventRow[]
    return rows.map((row) => ({
      seq: row.seq,
      accepted: row.accepted === 1,
      event: taskEventSchema.parse({
        taskId: row.task_id,
        source: row.source,
        type: row.type,
        at: row.at,
        payload: JSON.parse(row.payload),
      }),
    }))
  }

  private storeFiles(taskId: string, files: TaskFiles): void {
    for (const kind of ['task', 'log'] as const) {
      const stat = files[kind]
      if (stat !== undefined) {
        this.statements.upsertFile.run(taskId, kind, stat.mtimeMs, stat.size)
      }
    }
  }

  private transaction(work: () => void): void {
    this.db.exec('begin')
    try {
      work()
      this.db.exec('commit')
    } catch (error) {
      this.db.exec('rollback')
      throw error
    }
  }
}
