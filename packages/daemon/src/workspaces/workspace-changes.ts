import type {
  ChangeEntry,
  FileDiff,
  GitGraph,
  ImageSide,
  TreeEntry,
  WorkspaceFile,
} from '@natamux/core'
import { listChanges } from '../git/changes.js'
import { snapshotCommit } from '../git/commit-push.js'
import { currentBranch } from '../git/exec.js'
import { readFileDiff, readFileImage } from '../git/file-diff.js'
import { readGitGraph } from '../git/graph.js'
import { hasCommit, restoreFromSnapshot, rollbackFiles, rollbackPaths } from '../git/rollback.js'
import { TtlCache } from '../shared/ttl-cache.js'
import type { TaskService } from '../tasks/task-service.js'
import { FileNotChangedError, WorkspaceConflictError } from './workspace-errors.js'
import { listFolder, readWorkspaceFile, writeWorkspaceFile } from './workspace-files.js'

// Dashboards ask for the changes, the graph and the diff stats on timers. Within this time they
// share one read, so git runs less often. A change made through this service is seen at once.
export const GIT_READ_MS = 5000

// What the reads use of the workspace service.
type WorkspaceChangesHost = {
  tasks: TaskService
  now: () => number
  workspaceOf(taskId: string): string
  repoDirOf(repo: string | null): string
  touch(taskId: string): void
  exclusive<T>(taskId: string, work: () => Promise<T>): Promise<T>
}

// What the undo of a rollback needs: the snapshot made just before it, and the paths it touched.
type LastRollback = { snapshot: string; paths: string[]; files: number }

// The files of a workspace: its changes, their diffs, its graph and the files explorer.
export class WorkspaceChanges {
  private readonly changesReads: TtlCache<ChangeEntry[]>
  private readonly graphReads: TtlCache<GitGraph>
  // Only the last rollback of each task can be undone. A daemon restart forgets it.
  private readonly rollbacks = new Map<string, LastRollback>()

  constructor(private readonly host: WorkspaceChangesHost) {
    this.changesReads = new TtlCache(GIT_READ_MS, host.now)
    this.graphReads = new TtlCache(GIT_READ_MS, host.now)
  }

  // The workspace of the task may have changed.
  forget(taskId: string): void {
    this.changesReads.forget(taskId)
    this.graphReads.forget(taskId)
  }

  // The files of the workspace that differ from its last commit, with the lines of each. `fresh`
  // reads git now, and the answer replaces the kept one.
  async changes(taskId: string, fresh = false): Promise<ChangeEntry[]> {
    const workspace = this.host.workspaceOf(taskId)
    if (fresh) {
      this.changesReads.forget(taskId)
    }
    return this.changesReads.get(taskId, () => listChanges(workspace, { counts: true }))
  }

  // Only a file that is in the list of changes can be read: this is what keeps the request from
  // reaching any other file.
  async fileDiff(taskId: string, path: string): Promise<FileDiff> {
    const workspace = this.host.workspaceOf(taskId)
    return readFileDiff(workspace, await this.changedFile(taskId, path))
  }

  // One side of a changed image, with the same guard as fileDiff.
  async fileImage(
    taskId: string,
    path: string,
    side: ImageSide,
  ): Promise<{ type: string; bytes: Buffer }> {
    const workspace = this.host.workspaceOf(taskId)
    return readFileImage(workspace, await this.changedFile(taskId, path), side)
  }

  // The files explorer: one folder, one file, and a save by hand.
  tree(taskId: string, path: string): Promise<TreeEntry[]> {
    return listFolder(this.host.workspaceOf(taskId), path)
  }

  readFile(taskId: string, path: string): Promise<WorkspaceFile> {
    return readWorkspaceFile(this.host.workspaceOf(taskId), path)
  }

  async writeFile(
    taskId: string,
    path: string,
    text: string,
    version: string,
  ): Promise<{ path: string; version: string }> {
    const saved = await writeWorkspaceFile(this.host.workspaceOf(taskId), path, text, version)
    this.host.touch(taskId)
    return saved
  }

  // Puts changed files back as they are in the last commit; a new file is deleted. Only files in
  // the list of changes can be named. A snapshot of the whole worktree comes first, for the undo.
  async rollback(taskId: string, paths: readonly string[]): Promise<{ files: number }> {
    return this.host.exclusive(taskId, async () => {
      const workspace = this.host.workspaceOf(taskId)
      if (!(await hasCommit(workspace))) {
        throw new WorkspaceConflictError('There is no commit to roll back to.')
      }
      const changes = await this.changes(taskId, true)
      const byPath = new Map(changes.map((entry) => [entry.path, entry]))
      const entries = [...new Set(paths)].map((path) => {
        const entry = byPath.get(path)
        if (entry === undefined) {
          throw new FileNotChangedError(path)
        }
        if (entry.status === 'C') {
          throw new WorkspaceConflictError(`${path} has a conflict. Resolve it first`)
        }
        return entry
      })
      const snapshot = await snapshotCommit(workspace, 'Natamux rollback')
      // Kept before the files change, so a rollback that stops halfway can be undone too.
      this.rollbacks.set(taskId, { snapshot, paths: rollbackPaths(entries), files: entries.length })
      try {
        await rollbackFiles(workspace, entries)
      } finally {
        this.host.touch(taskId)
      }
      return { files: entries.length }
    })
  }

  // Brings back the files of the last rollback as they were just before it.
  async undoRollback(taskId: string): Promise<{ files: number }> {
    return this.host.exclusive(taskId, async () => {
      const workspace = this.host.workspaceOf(taskId)
      const last = this.rollbacks.get(taskId)
      if (last === undefined) {
        throw new WorkspaceConflictError('Nothing to undo.')
      }
      this.rollbacks.delete(taskId)
      try {
        await restoreFromSnapshot(workspace, last.snapshot, last.paths)
      } finally {
        this.host.touch(taskId)
      }
      return { files: last.files }
    })
  }

  dropRollback(taskId: string): void {
    this.rollbacks.delete(taskId)
  }

  // The kept list first: the dashboard asks for the changes and a diff at the same moment. A file
  // that changed after that list was read gets one fresh read.
  private async changedFile(taskId: string, path: string): Promise<ChangeEntry> {
    const find = (entries: ChangeEntry[]) => entries.find((entry) => entry.path === path)
    const change = find(await this.changes(taskId)) ?? find(await this.changes(taskId, true))
    if (!change) {
      throw new FileNotChangedError(path)
    }
    return change
  }

  // The history of the task branch next to the branch that the main checkout is on. `fresh` as
  // for changes.
  async graph(taskId: string, fresh = false): Promise<GitGraph> {
    const workspace = this.host.workspaceOf(taskId)
    const repoDir = this.host.repoDirOf(this.host.tasks.getTask(taskId).repo)
    if (fresh) {
      this.graphReads.forget(taskId)
    }
    return this.graphReads.get(taskId, async () =>
      readGitGraph(workspace, await currentBranch(repoDir)),
    )
  }
}
