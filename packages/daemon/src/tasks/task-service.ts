import { readdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import {
  activeSteps,
  applyTaskEvent,
  completeEvent,
  type EventType,
  firstPosition,
  makesCycle,
  type PullRequestStatus,
  resolveSteps,
  type Step,
  splitTaskId,
  stepRefsOf,
  type Task,
  type TaskEvent,
  type TaskState,
  taskEventSchema,
  taskSchema,
} from '@natamux/core'
import type { EventBus } from '../shared/event-bus.js'
import { deleteEndedShells } from '../terminals/ended-shells.js'
import { deleteOpenShells } from '../terminals/open-shells.js'
import { type AttachmentFile, saveAttachments } from './attachments.js'
import { eventLogPath, type LoggedEvent } from './event-log.js'
import { archiveDir, listTaskIds, readTaskFile, taskFilePath } from './task-files.js'
import {
  backgroundSinceAfter,
  compareText,
  LEGACY_SETTINGS,
  moveTask,
  positionOf,
  type SettingsOf,
} from './task-helpers.js'
import type { StoredEvent, TaskIndex } from './task-index.js'
import {
  type ActivityOptions,
  type ApplyEventResult,
  applyEventInputSchema,
  createTaskInputSchema,
  EDITABLE_FIELDS,
  TaskEditError,
  TaskNotFoundError,
  updateTaskInputSchema,
  type WorkspaceChange,
} from './task-inputs.js'
import { TaskRecords } from './task-records.js'
import { TaskSync } from './task-sync.js'

type TaskServiceOptions = {
  dataDir: string
  index: TaskIndex
  bus: EventBus
  now?: () => Date
  // The settings of a repository (`.natamux.json`). Default: the legacy pipeline.
  settingsOf?: SettingsOf
  // In milliseconds, for the checks of the task files. Default: Date.now.
  clock?: () => number
}

export class TaskService {
  private readonly dataDir: string
  private readonly index: TaskIndex
  private readonly bus: EventBus
  private readonly now: () => Date
  readonly settingsOf: SettingsOf
  private readonly sync: TaskSync
  private readonly records: TaskRecords

  constructor(options: TaskServiceOptions) {
    this.dataDir = options.dataDir
    this.index = options.index
    this.bus = options.bus
    this.now = options.now ?? (() => new Date())
    this.settingsOf = options.settingsOf ?? (() => LEGACY_SETTINGS)
    this.sync = new TaskSync({
      dataDir: this.dataDir,
      index: this.index,
      bus: this.bus,
      clock: options.clock ?? Date.now,
    })
    // Through the service, so a spy on one of its methods sees the calls of the records too.
    this.records = new TaskRecords({
      bus: this.bus,
      sync: this.sync,
      getTask: (taskId) => this.getTask(taskId),
      stepsOf: (task) => this.stepsOf(task),
      applyEvent: (taskId, rawInput) => this.applyEvent(taskId, rawInput),
      timestamp: () => this.timestamp(),
    })
  }

  // Loads every task into memory and watches the task files (see TaskSync.init).
  init(): void {
    this.sync.init()
  }

  // Stops watching the task files (see TaskSync.close).
  close(): void {
    this.sync.close()
  }

  // The steps of a task, with each stage's settings from its repository.
  stepsOf(task: Pick<Task, 'stages' | 'repo'>): Step[] {
    return resolveSteps(task.stages, this.settingsOf(task.repo).stages)
  }

  // The attachments are written before the task is published, so its context stage, which starts
  // at once, finds them. A title that starts with an id, such as "ABC-12: Fix it", picks the id,
  // unless `splitId` is false.
  createTask(
    rawInput: unknown,
    attachments: readonly AttachmentFile[] = [],
    { splitId = true }: { splitId?: boolean } = {},
  ): Task {
    const parsed = createTaskInputSchema.parse(rawInput)
    const parent = parsed.parentId ? this.checkParent(parsed.parentId) : null
    // A subtask is part of its parent's work, so it is in the same project.
    const input = parent && !parsed.repo ? { ...parsed, repo: parent.repo } : parsed
    const chosen = splitId ? splitTaskId(input.title) : { id: null, title: input.title }
    if (chosen.id && (this.sync.tasks.has(chosen.id) || readTaskFile(this.dataDir, chosen.id))) {
      throw new TaskEditError(`Task id ${chosen.id} is already used`)
    }
    const at = this.timestamp()
    // Stages that are off stay in the settings, but a new task does not get them.
    const steps = activeSteps(this.settingsOf(input.repo ?? null).stages)
    const start = firstPosition(steps)
    const task = taskSchema.parse({
      ...input,
      id: chosen.id ?? this.nextTaskId(),
      title: chosen.title,
      stage: start.stage,
      state: start.state,
      lanes: start.lanes,
      stages: stepRefsOf(steps),
      stageSince: at,
      createdAt: at,
      updatedAt: at,
    })

    const created: TaskEvent = {
      taskId: task.id,
      source: 'system',
      type: 'task.created',
      at,
      payload: {},
    }
    this.sync.store(task, [{ event: created, accepted: true }])
    saveAttachments(this.dataDir, task.id, attachments)
    if (attachments.length > 0) {
      const added = attachments.map(
        ({ name, bytes }): LoggedEvent => ({
          event: {
            taskId: task.id,
            source: 'system',
            type: 'attachment.added',
            at,
            payload: { name, size: bytes.length },
          },
          accepted: true,
        }),
      )
      this.sync.store(task, added, false)
    }
    this.bus.publish({ kind: 'task.created', task })
    return task
  }

  getTask(taskId: string): Task {
    this.sync.checkFile(taskId)
    const task = this.findTask(taskId)
    if (!task) {
      throw new TaskNotFoundError(taskId)
    }
    return task
  }

  // Newest first, as the index lists them.
  listTasks(state?: TaskState): Task[] {
    this.sync.checkFolder()
    const all = [...this.sync.tasks.values()]
    const tasks = state === undefined ? all : all.filter((task) => task.state === state)
    return tasks.sort((a, b) => compareText(b.updatedAt, a.updatedAt) || compareText(a.id, b.id))
  }

  // Before init() the map holds only the tasks this service wrote or read, so a task is looked
  // for on disk too. Tests share a data folder between services this way.
  private findTask(taskId: string): Task | undefined {
    const task = this.sync.tasks.get(taskId)
    if (task !== undefined || this.sync.started) {
      return task
    }
    const read = readTaskFile(this.dataDir, taskId)
    if (read !== undefined) {
      this.sync.tasks.set(taskId, Object.freeze(read))
    }
    return read
  }

  listEvents(taskId: string): StoredEvent[] {
    this.getTask(taskId)
    return this.index.listEvents(taskId)
  }

  // Changes the details of a task. Logs `task.edited` with the names of the fields that changed,
  // and writes nothing when none did.
  updateTask(taskId: string, rawInput: unknown): Task {
    const task = this.getTask(taskId)
    const input = updateTaskInputSchema.parse(rawInput)
    if (input.dependsOn !== undefined) {
      this.checkDependencies(taskId, input.dependsOn)
    }
    const next = { ...task, ...input }
    const changed = EDITABLE_FIELDS.filter(
      (field) => JSON.stringify(next[field]) !== JSON.stringify(task[field]),
    )
    if (changed.length === 0) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'task.edited',
      at: this.timestamp(),
      payload: { fields: changed },
    }
    const updated: Task = taskSchema.parse({ ...next, updatedAt: event.at })

    this.sync.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Removes the task and everything stored with it. A workspace is refused here: the caller
  // removes that first, so a worktree is never left behind with no task.
  deleteTask(taskId: string): Task[] {
    const task = this.getTask(taskId)
    if (task.workspace !== null) {
      throw new TaskEditError(`${taskId} still has a workspace`)
    }
    const related = this.detach(taskId)
    this.removeStoredTask(taskId)
    this.bus.publish({ kind: 'task.deleted', taskId })
    return related
  }

  // A deleted task must not stay as a parent or a dependency of the tasks that remain.
  private detach(taskId: string): Task[] {
    const changed: Task[] = []
    for (const other of this.listTasks()) {
      if (other.id === taskId) {
        continue
      }
      const fields: string[] = []
      let parentId = other.parentId
      let dependsOn = other.dependsOn
      if (other.parentId === taskId) {
        parentId = null
        fields.push('parentId')
      }
      if (other.dependsOn.includes(taskId)) {
        dependsOn = other.dependsOn.filter((id) => id !== taskId)
        fields.push('dependsOn')
      }
      if (fields.length === 0) {
        continue
      }
      const event: TaskEvent = {
        taskId: other.id,
        source: 'system',
        type: 'task.edited',
        at: this.timestamp(),
        payload: { fields },
      }
      const updated = taskSchema.parse({ ...other, parentId, dependsOn, updatedAt: event.at })
      this.sync.store(updated, [{ event, accepted: true }])
      this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
      changed.push(updated)
    }
    return changed
  }

  private removeStoredTask(taskId: string): void {
    rmSync(taskFilePath(this.dataDir, taskId), { force: true })
    rmSync(eventLogPath(this.dataDir, taskId), { force: true })
    rmSync(archiveDir(this.dataDir, taskId), { recursive: true, force: true })
    deleteEndedShells(this.dataDir, taskId)
    deleteOpenShells(this.dataDir, taskId)
    this.removeLaunchScripts(taskId)
    this.index.deleteTask(taskId)
    this.sync.forget(taskId)
  }

  // Launch scripts are named agent-launch-T-0001.sh and agent-launch-T-0001-2.sh.
  private removeLaunchScripts(taskId: string): void {
    let names: string[]
    try {
      names = readdirSync(this.dataDir)
    } catch (error) {
      if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
        return
      }
      throw error
    }
    const exact = `agent-launch-${taskId}.sh`
    const prefix = `agent-launch-${taskId}-`
    for (const name of names) {
      if (name === exact || (name.startsWith(prefix) && name.endsWith('.sh'))) {
        rmSync(join(this.dataDir, name), { force: true })
      }
    }
  }

  // Subtasks are one level deep: the parent must exist and must not be a subtask itself.
  private checkParent(parentId: string): Task {
    const parent = this.findTask(parentId)
    if (!parent) {
      throw new TaskEditError(`No such task: ${parentId}`)
    }
    if (parent.parentId !== null) {
      throw new TaskEditError(`${parentId} is a subtask; a subtask cannot have subtasks`)
    }
    return parent
  }

  private checkDependencies(taskId: string, dependsOn: readonly string[]): void {
    const tasks = this.listTasks()
    const known = new Set(tasks.map((task) => task.id))
    const unknown = dependsOn.filter((id) => !known.has(id))
    if (unknown.length > 0) {
      throw new TaskEditError(`No such task: ${unknown.join(', ')}`)
    }
    if (makesCycle(taskId, dependsOn, tasks)) {
      throw new TaskEditError('These dependencies would make a loop. A task cannot wait for itself')
    }
  }

  applyEvent(taskId: string, rawInput: unknown): ApplyEventResult {
    const task = this.getTask(taskId)
    const input = applyEventInputSchema.parse(rawInput)
    const steps = this.stepsOf(task)
    // A button or `natamux ask` may leave out the stage; the log keeps the one it was about.
    const event: TaskEvent = completeEvent(
      positionOf(task),
      taskEventSchema.parse({ ...input, taskId, at: this.timestamp() }),
      steps,
    )

    const { position, accepted, moved } = applyTaskEvent(positionOf(task), event, steps)
    const updated: Task = accepted
      ? {
          ...moveTask(task, position, moved, event.at),
          backgroundSince: backgroundSinceAfter(task, event, position.state),
          updatedAt: event.at,
        }
      : task

    this.sync.store(updated, [{ event, accepted }], accepted)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted, moved })
    return { task: updated, accepted, moved }
  }

  // The record writers (see TaskRecords).
  recordWorkspace(taskId: string, change: WorkspaceChange): Task {
    return this.records.recordWorkspace(taskId, change)
  }

  stopAgents(taskId: string): Task {
    return this.records.stopAgents(taskId)
  }

  recordGit(
    taskId: string,
    type: 'git.committed' | 'git.pushed' | 'pr.opened',
    payload: Record<string, string>,
  ): Task {
    return this.records.recordGit(taskId, type, payload)
  }

  recordReviewComment(taskId: string, text: string): Task {
    return this.records.recordReviewComment(taskId, text)
  }

  recordAgentLaunch(
    taskId: string,
    agent: string,
    program?: string,
    stage?: string,
    restoredIn?: string,
  ): Task {
    return this.records.recordAgentLaunch(taskId, agent, program, stage, restoredIn)
  }

  recordPullRequestStatus(taskId: string, status: PullRequestStatus): Task {
    return this.records.recordPullRequestStatus(taskId, status)
  }

  linkIssue(taskId: string, rawLink: unknown, source: TaskEvent['source'] = 'human'): Task {
    return this.records.linkIssue(taskId, rawLink, source)
  }

  setWorkflow(taskId: string, stageId: string, on: boolean): Task {
    return this.records.setWorkflow(taskId, stageId, on)
  }

  setCi(taskId: string, stageId: string, on: boolean): Task {
    return this.records.setCi(taskId, stageId, on)
  }

  setSkip(taskId: string, stageId: string, on: boolean): Task {
    return this.records.setSkip(taskId, stageId, on)
  }

  setWithoutAgent(taskId: string, on: boolean): void {
    this.records.setWithoutAgent(taskId, on)
  }

  recordEvent(
    taskId: string,
    type: EventType,
    payload: Record<string, unknown>,
    source: TaskEvent['source'] = 'system',
  ): Task {
    return this.records.recordEvent(taskId, type, payload, source)
  }

  recordActivity(taskId: string, options?: ActivityOptions): boolean {
    return this.records.recordActivity(taskId, options)
  }

  private timestamp(): string {
    return this.now().toISOString()
  }

  private nextTaskId(): string {
    // Ids the user chose with another prefix do not move the default numbers. A task whose file is
    // gone stays in memory, so its id counts too.
    const highest = [...this.sync.tasks.keys(), ...listTaskIds(this.dataDir)]
      .filter((id) => id.startsWith('T-'))
      .reduce((max, id) => Math.max(max, Number(id.slice(2))), 0)
    return `T-${String(highest + 1).padStart(4, '0')}`
  }
}
