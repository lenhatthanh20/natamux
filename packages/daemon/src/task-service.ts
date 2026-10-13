import { readdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import {
  activeSteps,
  applyTaskEvent,
  canRunWorkflow,
  completeEvent,
  DEFAULT_PROJECT_SETTINGS,
  type EventType,
  firstPosition,
  type IssueLink,
  issueLinkSchema,
  LEGACY_PIPELINE,
  makesCycle,
  offersCi,
  offersWorkflow,
  type ProjectSettings,
  type PullRequestStatus,
  RECORD_ONLY_EVENT_TYPES,
  resolveSteps,
  type StagePosition,
  type Step,
  splitTaskId,
  stageName,
  stagesOf,
  stepRefsOf,
  TASK_PRIORITIES,
  type Task,
  type TaskEvent,
  type TaskState,
  taskContextSchema,
  taskEventSchema,
  taskIdSchema,
  taskSchema,
} from '@natamux/core'
import { z } from 'zod'
import { type AttachmentFile, saveAttachments } from './attachments.js'
import type { EventBus } from './event-bus.js'
import {
  appendEventLine,
  eventLogPath,
  type LoggedEvent,
  readEventLog,
  writeEventLog,
} from './event-log.js'
import { archiveDir } from './stage-files.js'
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
import type { StoredEvent, TaskFiles, TaskIndex } from './task-index.js'

export const createTaskInputSchema = taskSchema
  .pick({
    title: true,
    description: true,
    priority: true,
    labels: true,
    agent: true,
    dependsOn: true,
    parentId: true,
    repo: true,
  })
  .extend({ context: taskContextSchema.nullable().optional() })

// The fields a person may change after a task is created. State has its own events.
// Built without the defaults of taskSchema, so a field that is left out stays as it is.
export const updateTaskInputSchema = z
  .object({
    title: taskSchema.shape.title,
    description: z.string(),
    priority: z.enum(TASK_PRIORITIES),
    labels: z.array(z.string().trim().min(1)),
    dependsOn: z.array(taskIdSchema),
  })
  .partial()
  .strict()

const EDITABLE_FIELDS = ['title', 'description', 'priority', 'labels', 'dependsOn'] as const

// How often a running task's sign of life is written down at most.
export const ACTIVITY_INTERVAL_MS = 30_000

// How often a sign of life alone rewrites the task file at most. The index gets every one.
const ACTIVITY_FILE_INTERVAL_MS = 60_000

// How often a read looks at the task files at most, for an edit by hand that the watcher missed:
// some file systems give no events. getTask stats one file; listTasks reads the folder.
const STAT_CHECK_MS = 1000
const LIST_CHECK_MS = 2000

export type ActivityOptions = { minIntervalMs?: number; endsWait?: boolean }

export const applyEventInputSchema = taskEventSchema
  .pick({ source: true, type: true, payload: true })
  .refine((input) => !RECORD_ONLY_EVENT_TYPES.includes(input.type), {
    message: 'This event is recorded by the daemon, not sent by clients',
    path: ['type'],
  })

export type CreateTaskInput = z.input<typeof createTaskInputSchema>
export type ApplyEventInput = z.input<typeof applyEventInputSchema>

export type ApplyEventResult = {
  task: Task
  accepted: boolean
  moved: boolean
}

export type WorkspaceChange = {
  workspace: string | null
  branch: string | null
}

export class TaskNotFoundError extends Error {
  readonly taskId: string

  constructor(taskId: string) {
    super(`Task ${taskId} does not exist`)
    this.name = 'TaskNotFoundError'
    this.taskId = taskId
  }
}

export type SettingsOf = (repo: string | null) => ProjectSettings

// A done or cancelled task needs its shells no more. Neither does a task that is gone.
export function isTaskEnded(
  tasks: { getTask(taskId: string): Pick<Task, 'state'> },
  taskId: string,
): boolean {
  try {
    const { state } = tasks.getTask(taskId)
    return state === 'done' || state === 'cancelled'
  } catch {
    return true
  }
}

// The flow tasks had before stages: implement, review, release. The daemon passes the settings of
// each repository instead.
const LEGACY_SETTINGS: ProjectSettings = { ...DEFAULT_PROJECT_SETTINGS, stages: LEGACY_PIPELINE }

// Compares as SQLite compares text, so the order stays the one the index gave.
function compareText(a: string, b: string): number {
  if (a === b) {
    return 0
  }
  return a < b ? -1 : 1
}

function positionOf(task: Pick<Task, 'stage' | 'state' | 'lanes'>): StagePosition {
  return { stage: task.stage, state: task.state, lanes: task.lanes }
}

// The task at its new position. Entering a new step starts its clock.
function moveTask(task: Task, position: StagePosition, moved: boolean, at: string): Task {
  return {
    ...task,
    stage: position.stage,
    state: position.state,
    lanes: position.lanes,
    stageSince: moved ? at : task.stageSince,
  }
}

// An agent waits for its background work from its first `agent.waiting` until it shows up again,
// or the task leaves running. Other events, such as checks that pass, do not end the wait.
function backgroundSinceAfter(task: Task, event: TaskEvent, state: TaskState): string | null {
  if (event.type === 'agent.waiting') {
    return task.backgroundSince ?? event.at
  }
  return state !== 'running' || event.source === 'agent' ? null : task.backgroundSince
}

export class TaskEditError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'TaskEditError'
  }
}

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
  // Every task by id, so a read touches no disk. A change replaces the task, never edits it;
  // each is frozen, so a caller that gets one cannot change the map either.
  private readonly tasks = new Map<string, Task>()
  // The stat of each task file as this service last wrote or read it. Tells its own writes from
  // edits by hand.
  private readonly fileStats = new Map<string, FileStat>()
  // The `updatedAt` of each task when its file was last written.
  private readonly writtenAt = new Map<string, string>()
  private readonly clock: () => number
  // When each task file was last looked at for edits by hand, and the folder for new files.
  private readonly checkedAt = new Map<string, number>()
  private folderCheckedAt = 0
  private stopWatching: (() => void) | undefined
  private started = false

  constructor(options: TaskServiceOptions) {
    this.dataDir = options.dataDir
    this.index = options.index
    this.bus = options.bus
    this.now = options.now ?? (() => new Date())
    this.settingsOf = options.settingsOf ?? (() => LEGACY_SETTINGS)
    this.clock = options.clock ?? Date.now
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
      const task = cached ?? readTaskFile(this.dataDir, id)
      if (task === undefined || stat === undefined) {
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
  private checkFile(taskId: string): void {
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
  private checkFolder(): void {
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

  // The steps of a task, with each stage's settings from its repository.
  stepsOf(task: Pick<Task, 'stages' | 'repo'>): Step[] {
    return resolveSteps(task.stages, this.settingsOf(task.repo).stages)
  }

  // Stores a change of a task: its file, its event log, the map, then the index in one
  // transaction. A crash in between loses nothing: the next start reads the files the index did
  // not see. Without `writeFile` the file keeps the task as it was.
  private store(task: Task, events: readonly LoggedEvent[], writeFile = true): void {
    const files: TaskFiles = {}
    if (writeFile) {
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

  // The attachments are written before the task is published, so its context stage, which starts
  // at once, finds them. A title that starts with an id, such as "ABC-12: Fix it", picks the id,
  // unless `splitId` is false.
  createTask(
    rawInput: unknown,
    attachments: readonly AttachmentFile[] = [],
    { splitId = true }: { splitId?: boolean } = {},
  ): Task {
    const input = createTaskInputSchema.parse(rawInput)
    if (input.parentId) {
      this.checkParent(input.parentId)
    }
    const chosen = splitId ? splitTaskId(input.title) : { id: null, title: input.title }
    if (chosen.id && readTaskFile(this.dataDir, chosen.id)) {
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
    this.store(task, [{ event: created, accepted: true }])
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
      this.store(task, added, false)
    }
    this.bus.publish({ kind: 'task.created', task })
    return task
  }

  getTask(taskId: string): Task {
    this.checkFile(taskId)
    const task = this.findTask(taskId)
    if (!task) {
      throw new TaskNotFoundError(taskId)
    }
    return task
  }

  // Newest first, as the index lists them.
  listTasks(state?: TaskState): Task[] {
    this.checkFolder()
    const all = [...this.tasks.values()]
    const tasks = state === undefined ? all : all.filter((task) => task.state === state)
    return tasks.sort((a, b) => compareText(b.updatedAt, a.updatedAt) || compareText(a.id, b.id))
  }

  // Before init() the map holds only the tasks this service wrote or read, so a task is looked
  // for on disk too. Tests share a data folder between services this way.
  private findTask(taskId: string): Task | undefined {
    const task = this.tasks.get(taskId)
    if (task !== undefined || this.started) {
      return task
    }
    const read = readTaskFile(this.dataDir, taskId)
    if (read !== undefined) {
      this.tasks.set(taskId, Object.freeze(read))
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

    this.store(updated, [{ event, accepted: true }])
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
      this.store(updated, [{ event, accepted: true }])
      this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
      changed.push(updated)
    }
    return changed
  }

  private removeStoredTask(taskId: string): void {
    rmSync(taskFilePath(this.dataDir, taskId), { force: true })
    rmSync(eventLogPath(this.dataDir, taskId), { force: true })
    rmSync(archiveDir(this.dataDir, taskId), { recursive: true, force: true })
    this.removeLaunchScripts(taskId)
    this.index.deleteTask(taskId)
    this.tasks.delete(taskId)
    this.fileStats.delete(taskId)
    this.writtenAt.delete(taskId)
    this.checkedAt.delete(taskId)
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
  private checkParent(parentId: string): void {
    const parent = this.findTask(parentId)
    if (!parent) {
      throw new TaskEditError(`No such task: ${parentId}`)
    }
    if (parent.parentId !== null) {
      throw new TaskEditError(`${parentId} is a subtask; a subtask cannot have subtasks`)
    }
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

    this.store(updated, [{ event, accepted }], accepted)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted, moved })
    return { task: updated, accepted, moved }
  }

  // Records where the task's working copy lives. Called by the workspace service.
  recordWorkspace(taskId: string, change: WorkspaceChange): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'system',
      type: change.workspace === null ? 'workspace.removed' : 'workspace.created',
      at: this.timestamp(),
      payload: { workspace: change.workspace ?? task.workspace, branch: change.branch },
    }
    const updated: Task = {
      ...task,
      workspace: change.workspace,
      branch: change.branch,
      updatedAt: event.at,
    }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Natamux ended the agents of the task, for example with its workspace. A stage whose agent ran
  // or waited for you would stay so with no agent, and would show up in Needs you later.
  stopAgents(taskId: string): Task {
    const task = this.getTask(taskId)
    const lanes: [string, TaskState][] =
      Object.keys(task.lanes).length > 0 ? Object.entries(task.lanes) : [[task.stage, task.state]]
    for (const [stage, state] of lanes) {
      if (state === 'running' || state === 'needs-input') {
        this.applyEvent(taskId, { source: 'system', type: 'agent.stopped', payload: { stage } })
      }
    }
    return this.getTask(taskId)
  }

  // Writes down a commit, a push or a new pull request. A pull request also sets `pr` of the task.
  recordGit(
    taskId: string,
    type: 'git.committed' | 'git.pushed' | 'pr.opened',
    payload: Record<string, string>,
  ): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = { taskId, source: 'git', type, at: this.timestamp(), payload }
    const updated: Task = {
      ...task,
      pr: type === 'pr.opened' && payload.url ? payload.url : task.pr,
      updatedAt: event.at,
    }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down a review comment that was sent to the agent of the task.
  recordReviewComment(taskId: string, text: string): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'review.commented',
      at: this.timestamp(),
      payload: { text },
    }
    const updated: Task = { ...task, updatedAt: event.at }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down that an agent was started in the task's terminal, and names it on the task.
  // The program is the command the shell looks for, to tell when it was not found.
  // The agent of a stage that starts its own (`stage`) is not the task's agent.
  recordAgentLaunch(taskId: string, agent: string, program?: string, stage?: string): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'agent.launched',
      at: this.timestamp(),
      payload: {
        agent,
        ...(program === undefined ? {} : { program }),
        ...(stage === undefined ? {} : { stage }),
      },
    }
    const updated: Task = {
      ...task,
      agent: stage === undefined ? agent : task.agent,
      updatedAt: event.at,
    }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Keeps what GitHub says about the task's pull request. Logs only what changed: the pull request
  // was merged (which finishes a task in review) or closed, or its checks passed or failed.
  recordPullRequestStatus(taskId: string, status: PullRequestStatus): Task {
    const task = this.getTask(taskId)
    const before = task.prStatus
    if (before?.state === status.state && before.checks === status.checks) {
      return task
    }

    const at = this.timestamp()
    const types: EventType[] = []
    if (status.checks !== before?.checks && status.checks === 'passing') {
      types.push('ci.passed')
    }
    if (status.checks !== before?.checks && status.checks === 'failing') {
      types.push('ci.failed')
    }
    if (status.state !== before?.state && status.state === 'merged') {
      types.push('pr.merged')
    }
    if (status.state !== before?.state && status.state === 'closed') {
      types.push('pr.closed')
    }

    const steps = this.stepsOf(task)
    let moving = task
    const logged: { event: TaskEvent; accepted: boolean }[] = []
    for (const type of types) {
      const event: TaskEvent = { taskId, source: 'git', type, at, payload: { url: task.pr } }
      const applied = applyTaskEvent(positionOf(moving), event, steps)
      moving = moveTask(moving, applied.position, applied.moved, at)
      logged.push({ event, accepted: applied.accepted })
    }
    const updated: Task = { ...moving, prStatus: status, updatedAt: at }

    this.store(updated, logged)
    for (const { event, accepted } of logged) {
      this.bus.publish({
        kind: 'task.updated',
        task: updated,
        event,
        accepted,
        moved: updated.stage !== task.stage,
      })
    }
    if (logged.length === 0) {
      this.bus.publish({ kind: 'task.changed', task: updated })
    }
    return updated
  }

  // Links the task to the item that tracks it elsewhere: a GitHub issue, a Jira ticket.
  linkIssue(taskId: string, rawLink: unknown, source: TaskEvent['source'] = 'human'): Task {
    const task = this.getTask(taskId)
    const issue: IssueLink = issueLinkSchema.parse(rawLink)
    const event: TaskEvent = {
      taskId,
      source,
      type: 'issue.linked',
      at: this.timestamp(),
      payload: { ...issue },
    }
    const updated: Task = { ...task, issue, updatedAt: event.at }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Chooses whether a stage of the task runs as a workflow. The stage runner reads the choice the
  // next time the stage starts, so a running stage keeps its prompt. Turning it off always works.
  setWorkflow(taskId: string, stageId: string, on: boolean): Task {
    const task = this.getTask(taskId)
    const stages = this.stepsOf(task).flatMap(stagesOf)
    const stage = stages.find((each) => each.id === stageId)
    if (stage === undefined) {
      throw new TaskEditError(`${taskId} has no stage "${stageId}"`)
    }
    if (on && !offersWorkflow(stage)) {
      throw new TaskEditError(
        `The ${stageName(stage)} stage cannot run with a workflow. Turn it on for the stage in the project settings first`,
      )
    }
    if (on && !canRunWorkflow(task.agent)) {
      throw new TaskEditError(`Only Claude Code can run a workflow; ${taskId} uses ${task.agent}`)
    }
    const workflowStages = on
      ? [...new Set([...task.workflowStages, stageId])]
      : task.workflowStages.filter((id) => id !== stageId)
    if (workflowStages.length === task.workflowStages.length) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'stage.workflow-set',
      at: this.timestamp(),
      payload: { stage: stageId, on },
    }
    const updated: Task = { ...task, workflowStages, updatedAt: event.at }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Whether the stage waits for CI before it passes. The project settings must offer it.
  setCi(taskId: string, stageId: string, on: boolean): Task {
    const task = this.getTask(taskId)
    const stages = this.stepsOf(task).flatMap(stagesOf)
    const stage = stages.find((each) => each.id === stageId)
    if (stage === undefined) {
      throw new TaskEditError(`${taskId} has no stage "${stageId}"`)
    }
    if (on && !offersCi(stage)) {
      throw new TaskEditError(
        `The ${stageName(stage)} stage cannot be checked on CI. Turn it on for the stage in the project settings first`,
      )
    }
    const before = task.ciStages ?? []
    const ciStages = on ? [...new Set([...before, stageId])] : before.filter((id) => id !== stageId)
    if (ciStages.length === before.length) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'stage.ci-set',
      at: this.timestamp(),
      payload: { stage: stageId, on },
    }
    const updated: Task = { ...task, ciStages, updatedAt: event.at }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down an event that changes no state, such as fetched context.
  recordEvent(
    taskId: string,
    type: EventType,
    payload: Record<string, unknown>,
    source: TaskEvent['source'] = 'system',
  ): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = { taskId, source, type, at: this.timestamp(), payload }
    const updated: Task = { ...task, updatedAt: event.at }

    this.store(updated, [{ event, accepted: true }])
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down that the agent of a running task is still at work, so the time of its last update
  // is the time of its last sign of life. At most once per `minIntervalMs`, so a busy agent does
  // not fill the log, except that the first sign of life ends a wait for background work at once.
  // Terminal output alone is no such sign: a redraw after a resize prints too while the agent
  // still waits. So with `endsWait: false` the wait stays. Returns whether it wrote anything.
  recordActivity(
    taskId: string,
    { minIntervalMs = ACTIVITY_INTERVAL_MS, endsWait = true }: ActivityOptions = {},
  ): boolean {
    const task = this.getTask(taskId)
    if (task.state !== 'running') {
      return false
    }

    const at = this.timestamp()
    const endsWaitNow = endsWait && task.backgroundSince !== null
    if (!endsWaitNow && Date.parse(at) - Date.parse(task.updatedAt) < minIntervalMs) {
      return false
    }

    const event: TaskEvent = { taskId, source: 'agent', type: 'agent.active', at, payload: {} }
    const updated: Task = {
      ...task,
      backgroundSince: endsWait ? null : task.backgroundSince,
      updatedAt: at,
    }
    // When only the time moves, the file can wait: the next real change writes it anyway.
    const writtenAt = this.writtenAt.get(taskId)
    const writeFile =
      updated.backgroundSince !== task.backgroundSince ||
      writtenAt === undefined ||
      Date.parse(at) - Date.parse(writtenAt) >= ACTIVITY_FILE_INTERVAL_MS

    this.store(updated, [{ event, accepted: true }], writeFile)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return true
  }

  private timestamp(): string {
    return this.now().toISOString()
  }

  private nextTaskId(): string {
    // Ids the user chose with another prefix do not move the default numbers.
    const highest = listTaskIds(this.dataDir)
      .filter((id) => id.startsWith('T-'))
      .reduce((max, id) => Math.max(max, Number(id.slice(2))), 0)
    return `T-${String(highest + 1).padStart(4, '0')}`
  }
}
