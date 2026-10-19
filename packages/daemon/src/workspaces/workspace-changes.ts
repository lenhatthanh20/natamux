import type {
  ChangeEntry,
  FileDiff,
  GitGraph,
  ImageSide,
  TreeEntry,
  WorkspaceFile,
} from '@natamux/core'
import { listChanges } from '../git/changes.js'
import { currentBranch } from '../git/exec.js'
import { readFileDiff, readFileImage } from '../git/file-diff.js'
import { readGitGraph } from '../git/graph.js'
import { TtlCache } from '../shared/ttl-cache.js'
import type { TaskService } from '../tasks/task-service.js'
import { FileNotChangedError } from './workspace-errors.js'
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
}

// The files of a workspace: its changes, their diffs, its graph and the files explorer.
export class WorkspaceChanges {
  private readonly changesReads: TtlCache<ChangeEntry[]>
  private readonly graphReads: TtlCache<GitGraph>

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
