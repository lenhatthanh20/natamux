import type { DiffStat, Task } from '@natamux/core'
import { readDiffStat } from '../git/changes.js'
import { currentBranch } from '../git/exec.js'
import { startRef } from '../git/graph.js'
import { TtlCache } from '../shared/ttl-cache.js'
import type { TaskService } from '../tasks/task-service.js'
import { mapLimit } from './map-limit.js'
import { GIT_READ_MS } from './workspace-changes.js'

const DIFF_STAT_READS = 4
// The diff stat of a workspace is read again when the workspace may have changed (see touch). A
// change that no hook, shell or commit here saw, such as one in another editor, shows after this.
export const DIFF_STATS_MAX_AGE_MS = 5 * 60_000

type Entry = readonly [string, DiffStat]

// What the diff of a workspace counts from: where a new workspace starts, by name.
async function baseForDiff(repoDir: string): Promise<string | null> {
  const branch = await currentBranch(repoDir)
  return branch === null ? null : ((await startRef(repoDir, branch)) ?? branch)
}

// The size of the work in each workspace, read again only for the workspaces that changed.
export class DiffStats {
  private readonly reads: TtlCache<Record<string, DiffStat>>
  // The last diff stat of each workspace, and the tasks whose workspace changed since.
  private readonly stats = new Map<string, { stat: DiffStat; readAt: number; round: number }>()
  private readonly dirtyStats = new Set<string>()
  private statsRound = 0

  constructor(
    private readonly tasks: TaskService,
    private readonly repoDirOf: (repo: string | null) => string,
    private readonly now: () => number,
  ) {
    this.reads = new TtlCache(GIT_READ_MS, now)
  }

  // A workspace that git cannot read is left out.
  get(): Promise<Record<string, DiffStat>> {
    return this.reads.get('', () => this.read())
  }

  // The workspace of the task may have changed.
  touch(taskId: string): void {
    this.dirtyStats.add(taskId)
    // The diff stats of the other tasks still come from `stats`: only this one runs git again.
    this.reads.forget()
  }

  private async read(): Promise<Record<string, DiffStat>> {
    const baseOf = this.baseReader()
    const withWorkspace = this.tasks.listTasks().filter((task) => task.workspace !== null)
    const readAt = this.now()
    this.statsRound += 1
    const round = this.statsRound
    // Two git commands per workspace: with many workspaces, all at once would load every core.
    const entries = await mapLimit(withWorkspace, DIFF_STAT_READS, (task) =>
      this.readOne(task, baseOf, readAt, round),
    )
    this.prune(withWorkspace)
    return Object.fromEntries(entries.flat())
  }

  // Where a new workspace starts, so the commits the checkout did not pull do not count. Read
  // once per repository and read of the diff stats.
  private baseReader(): (repo: string | null) => Promise<string | null> {
    const bases = new Map<string | null, Promise<string | null>>()
    return (repo) => {
      const known = bases.get(repo) ?? baseForDiff(this.repoDirOf(repo))
      bases.set(repo, known)
      return known
    }
  }

  private async readOne(
    task: Task,
    baseOf: (repo: string | null) => Promise<string | null>,
    readAt: number,
    round: number,
  ): Promise<Entry[]> {
    const known = this.stats.get(task.id)
    if (
      known !== undefined &&
      !this.dirtyStats.has(task.id) &&
      readAt - known.readAt < DIFF_STATS_MAX_AGE_MS
    ) {
      return [[task.id, known.stat] as const]
    }
    // Before the read: a change while git runs marks the task again.
    this.dirtyStats.delete(task.id)
    try {
      const stat = await readDiffStat(task.workspace ?? '', await baseOf(task.repo))
      if (!this.newer(task.id, round)) {
        this.stats.set(task.id, { stat, readAt, round })
      }
      return [[task.id, stat] as const]
    } catch {
      if (!this.newer(task.id, round)) {
        this.stats.delete(task.id)
      }
      return []
    }
  }

  // A touch during a read starts another one. When that newer read ends first, the older one must
  // not put back its older stat.
  private newer(taskId: string, round: number): boolean {
    return (this.stats.get(taskId)?.round ?? 0) > round
  }

  // A workspace that is gone keeps no stat.
  private prune(withWorkspace: readonly Task[]): void {
    const ids = new Set(withWorkspace.map((task) => task.id))
    for (const id of this.stats.keys()) {
      if (!ids.has(id)) {
        this.stats.delete(id)
      }
    }
  }
}
