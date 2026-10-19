import type { Task } from '@natamux/core'
import type { EventBus } from '../shared/event-bus.js'
import {
  appendEventLine,
  eventLogPath,
  type LoggedEvent,
  readEventLog,
  writeEventLog,
} from './event-log.js'
import {
  type FileStat,
  listTaskIds,
  readTaskFile,
  sameStat,
  statFile,
  taskFilePath,
  watchTaskFiles,
  writeTaskFile,
} from './task-files.js'
import { LIST_CHECK_MS, STAT_CHECK_MS } from './task-helpers.js'
import type { TaskFiles, TaskIndex } from './task-index.js'
import { TaskFileChangedError } from './task-inputs.js'

type TaskSyncOptions = {
  dataDir: string
  index: TaskIndex
  bus: EventBus
  clock: () => number
}

// The tasks in memory, kept in line with the task files, their event logs and the index.
export class TaskSync {
  private readonly dataDir: string
  private readonly index: TaskIndex
  private readonly bus: EventBus
  // Every task by id, so a read touches no disk. A change replaces the task, never edits it;
  // each is frozen, so a caller that gets one cannot change the map either.
  readonly tasks = new Map<string, Task>()
  // The stat of each task file as this service last wrote or read it. Tells its own writes from
  // edits by hand.
  private readonly fileStats = new Map<string, FileStat>()
  // The `updatedAt` of each task when its file was last written.
  readonly writtenAt = new Map<string, string>()
  private readonly clock: () => number
  // When each task file was last looked at for edits by hand, and the folder for new files.
  private readonly checkedAt = new Map<string, number>()
  private folderCheckedAt = 0
  private stopWatching: (() => void) | undefined
  started = false

  constructor(options: TaskSyncOptions) {
    this.dataDir = options.dataDir
    this.index = options.index
    this.bus = options.bus
    this.clock = options.clock
  }

  // Loads every task into memory and brings the index in line with the files. A file with the
  // time and size the index saw last is not read again, so a restart reads only what changed.
  // Then watches the task files for edits by hand, until close().
  init(): void {
    const known = this.index.taskFiles()
    const indexed = this.indexedTasks()
    const files = new Map<string, TaskFiles>()
    const now = this.clock()
    this.tasks.clear()
    this.fileStats.clear()
    this.checkedAt.clear()
    this.folderCheckedAt = now
    for (const id of listTaskIds(this.dataDir)) {
      const stat = statFile(taskFilePath(this.dataDir, id))
      const cached = sameStat(stat, known.get(id)?.task) ? indexed.get(id) : undefined
      let task: Task | undefined
      try {
        task = cached ?? readTaskFile(this.dataDir, id)
      } catch (error) {
        // One broken file must not stop the daemon: the task as the index saw it last stays, or
        // none. The file is read again when it changes.
        console.error(`Could not read the task file of ${id}: ${String(error)}`)
        task = indexed.get(id)
      }
      if (task === undefined || stat === undefined) {
        if (stat !== undefined) {
          this.fileStats.set(id, stat)
        }
        continue
      }
      this.tasks.set(id, Object.freeze(task))
      this.fileStats.set(id, stat)
      this.checkedAt.set(id, now)
      files.set(id, { task: stat, log: this.syncEventLog(id, known.get(id)?.log) })
    }
    this.index.replaceAllTasks([...this.tasks.values()], files)
    this.started = true
    this.stopWatching?.()
    this.stopWatching = watchTaskFiles(this.dataDir, (ids) => {
      for (const id of ids) {
        this.tryReload(id)
      }
    })
  }

  // Stops watching the task files. The daemon never does; tests do, before they remove the files.
  close(): void {
    this.stopWatching?.()
    this.stopWatching = undefined
  }

  // The tasks as the index holds them. A row of an older version may not parse; then every file
  // is read.
  private indexedTasks(): Map<string, Task> {
    try {
      return new Map(this.index.listTasks().map((task) => [task.id, task]))
    } catch {
      return new Map()
    }
  }

  // Brings the events of a task in the index in line with its event log, when the log changed
  // since the index saw it. A task with no event log yet (made before there were logs) gets one
  // from the index. Returns the stat of the log.
  private syncEventLog(taskId: string, known: FileStat | undefined): FileStat | undefined {
    const path = eventLogPath(this.dataDir, taskId)
    const stat = statFile(path)
    if (sameStat(stat, known)) {
      return stat
    }
    const logged = readEventLog(this.dataDir, taskId)
    if (logged === undefined) {
      writeEventLog(this.dataDir, taskId, this.index.listEvents(taskId))
      return statFile(path)
    }
    if (logged.length !== this.index.countEvents(taskId)) {
      this.index.replaceEvents(taskId, logged)
    }
    return stat
  }

  // A task file changed on disk. Unless this service wrote it, the task is read again.
  private reload(taskId: string): void {
    const stat = statFile(taskFilePath(this.dataDir, taskId))
    if (sameStat(stat, this.fileStats.get(taskId))) {
      return
    }
    // A file gone from outside (an editor that saves by delete and write, a sync tool) keeps the
    // task: task.deleted would end its agents. Delete a task in Natamux to end it.
    if (stat === undefined) {
      return
    }
    // Before the read, so a broken file is read again only when it changes.
    this.fileStats.set(taskId, stat)
    const task = readTaskFile(this.dataDir, taskId)
    if (task === undefined) {
      return
    }
    this.tasks.set(taskId, Object.freeze(task))
    // Not task.created, even for a new file: that starts the context stage, and a file put there
    // by hand (or back from a backup) must not start an agent. A restart does not either.
    this.bus.publish({ kind: 'task.changed', task })
  }

  private tryReload(taskId: string): void {
    try {
      this.reload(taskId)
    } catch (error) {
      // A half-written or broken file: the last good task stays until the file is fixed.
      console.error(`Could not read the task file of ${taskId}: ${String(error)}`)
    }
  }

  // The watcher may miss an edit by hand, so a read stats the task file too, once a second at most.
  checkFile(taskId: string): void {
    if (!this.started || !this.tasks.has(taskId)) {
      return
    }
    const now = this.clock()
    if (now - (this.checkedAt.get(taskId) ?? 0) < STAT_CHECK_MS) {
      return
    }
    this.checkedAt.set(taskId, now)
    this.tryReload(taskId)
  }

  // The same for every task file, and files added by hand, every two seconds at most.
  checkFolder(): void {
    const now = this.clock()
    if (!this.started || now - this.folderCheckedAt < LIST_CHECK_MS) {
      return
    }
    this.folderCheckedAt = now
    let ids: string[]
    try {
      ids = listTaskIds(this.dataDir)
    } catch (error) {
      // A read of the folder that fails (EMFILE, EACCES) must not fail listTasks: it has the tasks.
      console.error(`Could not read the task folder: ${String(error)}`)
      return
    }
    for (const id of ids) {
      this.checkedAt.set(id, now)
      this.tryReload(id)
    }
  }

  // Stores a change of a task: its file, its event log, the map, then the index in one
  // transaction. A crash in between loses nothing: the next start reads the files the index did
  // not see. Without `writeFile` the file keeps the task as it was.
  store(task: Task, events: readonly LoggedEvent[], writeFile = true): void {
    const files: TaskFiles = {}
    if (writeFile) {
      this.refuseUnreadEdit(task.id)
      files.task = writeTaskFile(this.dataDir, task)
      this.fileStats.set(task.id, files.task)
      this.writtenAt.set(task.id, task.updatedAt)
    }
    for (const { event, accepted } of events) {
      files.log = appendEventLine(this.dataDir, event, accepted)
    }
    this.tasks.set(task.id, Object.freeze(task))
    this.index.record(task.id, { task, events, files })
  }

  // checkFile looks once a second and the watcher waits a moment, so the task in memory may be
  // older than an edit by hand. Then that edit is read and the change is refused. A missing file
  // is written as before. shortcut: an edit between this stat and the write is still lost; that
  // needs a file lock.
  private refuseUnreadEdit(taskId: string): void {
    if (!this.started) {
      return
    }
    const stat = statFile(taskFilePath(this.dataDir, taskId))
    if (stat === undefined || sameStat(stat, this.fileStats.get(taskId))) {
      return
    }
    this.checkedAt.set(taskId, this.clock())
    this.tryReload(taskId)
    throw new TaskFileChangedError(taskId)
  }

  // A deleted task leaves memory too.
  forget(taskId: string): void {
    this.tasks.delete(taskId)
    this.fileStats.delete(taskId)
    this.writtenAt.delete(taskId)
    this.checkedAt.delete(taskId)
  }
}
