import { DatabaseSync } from 'node:sqlite'
import {
  type Task,
  type TaskEvent,
  type TaskState,
  taskEventSchema,
  taskSchema,
} from '@helmsman/core'

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

export type StoredEvent = {
  seq: number
  accepted: boolean
  event: TaskEvent
}

type TaskRow = { data: string }

type EventRow = {
  seq: number
  task_id: string
  source: string
  type: string
  at: string
  payload: string
  accepted: number
}

// The task files are the source of truth. This index is a cache of them plus the event log.
export class TaskIndex {
  private readonly db: DatabaseSync

  constructor(path: string) {
    this.db = new DatabaseSync(path)
    this.db.exec('pragma journal_mode = wal')
    this.db.exec(SCHEMA)
  }

  close(): void {
    this.db.close()
  }

  upsertTask(task: Task): void {
    this.db.prepare(UPSERT_TASK).run({
      id: task.id,
      title: task.title,
      state: task.state,
      priority: task.priority,
      updatedAt: task.updatedAt,
      data: JSON.stringify(task),
    })
  }

  replaceAllTasks(tasks: readonly Task[]): void {
    this.db.exec('begin')
    try {
      this.db.exec('delete from tasks_index')
      for (const task of tasks) {
        this.upsertTask(task)
      }
      this.db.exec('commit')
    } catch (error) {
      this.db.exec('rollback')
      throw error
    }
  }

  listTasks(state?: TaskState): Task[] {
    const rows =
      state === undefined
        ? this.db.prepare('select data from tasks_index order by updated_at desc, id').all()
        : this.db
            .prepare('select data from tasks_index where state = ? order by updated_at desc, id')
            .all(state)
    return (rows as TaskRow[]).map((row) => taskSchema.parse(JSON.parse(row.data)))
  }

  appendEvent(event: TaskEvent, accepted: boolean): void {
    this.db
      .prepare(
        'insert into events (task_id, source, type, at, payload, accepted) values (?, ?, ?, ?, ?, ?)',
      )
      .run(
        event.taskId,
        event.source,
        event.type,
        event.at,
        JSON.stringify(event.payload),
        accepted ? 1 : 0,
      )
  }

  countEvents(taskId: string): number {
    const row = this.db
      .prepare('select count(*) as count from events where task_id = ?')
      .get(taskId) as { count: number }
    return row.count
  }

  // Puts the events of a task back from its event log, in order.
  replaceEvents(taskId: string, events: readonly { event: TaskEvent; accepted: boolean }[]): void {
    this.db.exec('begin')
    try {
      this.db.prepare('delete from events where task_id = ?').run(taskId)
      for (const { event, accepted } of events) {
        this.appendEvent(event, accepted)
      }
      this.db.exec('commit')
    } catch (error) {
      this.db.exec('rollback')
      throw error
    }
  }

  listEvents(taskId: string): StoredEvent[] {
    const rows = this.db
      .prepare('select * from events where task_id = ? order by seq')
      .all(taskId) as EventRow[]
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
}
