import { mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { realpath, stat } from 'node:fs/promises'
import { homedir } from 'node:os'
import { isAbsolute, join, relative, sep } from 'node:path'
import type { TaskUsage, UsageRow } from '@natamux/core'
import { z } from 'zod'
import type { EventBus } from '../shared/event-bus.js'
import { tasksDir } from '../tasks/task-files.js'
import { costOf } from './model-prices.js'
import {
  addCounts,
  COUNT_FIELDS,
  type ModelUsage,
  usageReader,
  zeroCounts,
} from './usage-readers.js'

const MAX_FILE_BYTES = 200 * 1024 * 1024
const USAGE_FILE = /^([A-Z]+-\d+)\.usage\.json$/
const SESSION_FILE = /\.jsonl?$/

const count = z.number().int().nonnegative()
const modelUsageSchema = z.record(
  z.string(),
  z.object({
    input: count,
    cacheRead: count,
    cacheWrite: count,
    cacheWrite1h: count,
    output: count,
  }),
)

// `files`: the last totals counted of each session file, so the next read adds only what is new.
// `stages`: what each stage used, by stage id ("" for none), then agent.
const storedUsageSchema = z.object({
  files: z.record(z.string(), z.object({ agent: z.string(), models: modelUsageSchema })),
  stages: z.record(z.string(), z.record(z.string(), modelUsageSchema)),
})

type StoredUsage = z.infer<typeof storedUsageSchema>

export function usagePath(dataDir: string, taskId: string): string {
  return join(tasksDir(dataDir), `${taskId}.usage.json`)
}

export function deleteUsage(dataDir: string, taskId: string): void {
  rmSync(usagePath(dataDir, taskId), { force: true })
}

function isInside(folder: string, path: string): boolean {
  const rest = relative(folder, path)
  return rest !== '' && rest !== '..' && !rest.startsWith(`..${sep}`) && !isAbsolute(rest)
}

// The path comes from the agent's hook. Null for a path that is not a session file in the home
// folder: no usage is read from it.
export async function sessionFile(path: string, home: string): Promise<string | null> {
  if (!isAbsolute(path) || !SESSION_FILE.test(path)) {
    return null
  }
  try {
    const real = await realpath(path)
    if (!SESSION_FILE.test(real) || !isInside(await realpath(home), real)) {
      return null
    }
    const info = await stat(real)
    return info.isFile() && info.size <= MAX_FILE_BYTES ? real : null
  } catch {
    return null
  }
}

function usageRows(stored: StoredUsage): UsageRow[] {
  return Object.entries(stored.stages).flatMap(([stage, agents]) =>
    Object.entries(agents).flatMap(([agent, models]) =>
      Object.entries(models).map(([model, counts]) => ({
        stage,
        agent,
        model,
        input: counts.input,
        cacheRead: counts.cacheRead,
        cacheWrite: counts.cacheWrite,
        output: counts.output,
        costUsd: costOf(counts, model),
      })),
    ),
  )
}

// Cost comes from the price table at each call, so a fixed price applies to old tasks too.
export function taskUsage(stored: StoredUsage): TaskUsage {
  const stages = usageRows(stored)
  const priced = stages.flatMap((row) => (row.costUsd === null ? [] : [row.costUsd]))
  return {
    tokens: stages.reduce(
      (sum, row) => sum + row.input + row.cacheRead + row.cacheWrite + row.output,
      0,
    ),
    costUsd: priced.length === 0 ? null : priced.reduce((sum, cost) => sum + cost, 0),
    unpricedModels: [
      ...new Set(stages.filter((row) => row.costUsd === null).map((row) => row.model)),
    ],
    stages,
  }
}

// What is in `now` and not yet in `before`, by model. Never below 0, as a file may start again.
function newUsage(now: ModelUsage, before: ModelUsage): ModelUsage {
  const added: ModelUsage = {}
  for (const [model, counts] of Object.entries(now)) {
    const change = zeroCounts()
    for (const field of COUNT_FIELDS) {
      change[field] = Math.max(0, counts[field] - (before[model]?.[field] ?? 0))
    }
    if (COUNT_FIELDS.some((field) => change[field] > 0)) {
      added[model] = change
    }
  }
  return added
}

type Seen = { taskId: string; stage: string; agent: string; transcript: string }

type UsageServiceOptions = {
  dataDir: string
  bus?: EventBus
  // The folder that session files must be in. The user's home folder by default.
  home?: string
}

// Reads the token usage of agent sessions after their turns, and keeps it per task and stage.
export class UsageService {
  private readonly dataDir: string
  private readonly bus: EventBus | undefined
  private readonly home: string
  private readonly usage = new Map<string, StoredUsage>()
  // The files read now, with the last hook that came during the read: it is read again after.
  private readonly reading = new Map<string, Seen | null>()
  // Tasks deleted while a read of theirs ran: that read must not bring their usage back.
  private readonly deleted = new Set<string>()
  private readonly pending = new Set<Promise<void>>()

  constructor(options: UsageServiceOptions) {
    this.dataDir = options.dataDir
    this.bus = options.bus
    this.home = options.home ?? homedir()
    this.load()
    this.bus?.subscribe((message) => {
      if (message.kind === 'task.deleted') {
        this.usage.delete(message.taskId)
        this.deleted.add(message.taskId)
      } else if (message.kind === 'task.created') {
        this.deleted.delete(message.task.id)
      }
    })
  }

  // Called after a turn of an agent in a task. Returns at once; the file is read in the background.
  seen(taskId: string, stage: string | null, agent: string, transcript: string): void {
    if (usageReader(agent) === undefined) {
      return
    }
    const next = { taskId, stage: stage ?? '', agent, transcript }
    if (this.reading.has(transcript)) {
      this.reading.set(transcript, next)
      return
    }
    this.reading.set(transcript, null)
    const work = this.readUntilDone(next).finally(() => this.pending.delete(work))
    this.pending.add(work)
  }

  usageOf(taskId: string): TaskUsage | null {
    const stored = this.usage.get(taskId)
    return stored === undefined ? null : taskUsage(stored)
  }

  all(): Record<string, TaskUsage> {
    return Object.fromEntries(
      [...this.usage].map(([taskId, stored]) => [taskId, taskUsage(stored)]),
    )
  }

  // Waits for the reads that run now. For tests.
  async settle(): Promise<void> {
    while (this.pending.size > 0) {
      await Promise.allSettled([...this.pending])
    }
  }

  private async readUntilDone(first: Seen): Promise<void> {
    let seen: Seen | null = first
    while (seen !== null) {
      try {
        await this.read(seen)
      } catch (error) {
        console.error(`Could not read the usage of ${seen.taskId}: ${String(error)}`)
      }
      seen = this.reading.get(first.transcript) ?? null
      this.reading.set(first.transcript, null)
    }
    this.reading.delete(first.transcript)
  }

  private async read({ taskId, stage, agent, transcript }: Seen): Promise<void> {
    const reader = usageReader(agent)
    const path = await sessionFile(transcript, this.home)
    if (reader === undefined || path === null) {
      return
    }
    const now = await reader(path)
    if (this.deleted.has(taskId)) {
      return
    }
    const stored = this.usage.get(taskId) ?? { files: {}, stages: {} }
    const added = newUsage(now, stored.files[transcript]?.models ?? {})
    if (Object.keys(added).length === 0) {
      return
    }
    const agents = stored.stages[stage] ?? {}
    const models = agents[agent] ?? {}
    for (const [model, counts] of Object.entries(added)) {
      addCounts(models, model, counts)
    }
    agents[agent] = models
    stored.stages[stage] = agents
    stored.files[transcript] = { agent, models: now }
    this.usage.set(taskId, stored)
    this.save(taskId, stored)
    this.bus?.publish({ kind: 'usage.changed', taskId, usage: taskUsage(stored) })
  }

  private save(taskId: string, stored: StoredUsage): void {
    mkdirSync(tasksDir(this.dataDir), { recursive: true })
    const path = usagePath(this.dataDir, taskId)
    const temporaryPath = `${path}.tmp`
    writeFileSync(temporaryPath, JSON.stringify(stored))
    renameSync(temporaryPath, path)
  }

  // A missing or broken file reads as no usage.
  private load(): void {
    let names: string[]
    try {
      names = readdirSync(tasksDir(this.dataDir))
    } catch {
      return
    }
    for (const name of names) {
      const taskId = USAGE_FILE.exec(name)?.[1]
      if (taskId === undefined) {
        continue
      }
      try {
        const text = readFileSync(join(tasksDir(this.dataDir), name), 'utf8')
        this.usage.set(taskId, storedUsageSchema.parse(JSON.parse(text)))
      } catch {
        // No usage for this task.
      }
    }
  }
}
