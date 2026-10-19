import type { CommitMessageDraft, Task } from '@natamux/core'
import { isLaunchable, type LaunchableAgent } from '../agents/agent-launch.js'
import { listChanges } from '../git/changes.js'
import { draftCommitMessage, MAX_DIFF_CHARS, type RunAgentOnce } from '../git/commit-drafter.js'
import { commitAll, pushBranch, readCommitDiff } from '../git/commit-push.js'
import { createPullRequest, type RunGh } from '../git/gh.js'
import type { TaskService } from '../tasks/task-service.js'
import { WorkspaceConflictError } from './workspace-errors.js'

// What commit, push and pull request use of the workspace service.
type WorkspacePublishHost = {
  tasks: TaskService
  gh: RunGh
  pullRequestBody: (task: Task) => Promise<string>
  programOf: (agent: LaunchableAgent) => string
  runAgentOnce: RunAgentOnce
  log: (message: string) => void
  workspaceOf(taskId: string): string
  exclusive<T>(taskId: string, work: () => Promise<T>): Promise<T>
}

// Takes the work of a workspace out: its commits, the pushes of its branch, its pull request.
export class WorkspacePublish {
  private readonly drafts = new Map<string, Promise<CommitMessageDraft>>()

  constructor(private readonly host: WorkspacePublishHost) {}

  // Commits every change of the workspace on the task branch.
  async commit(taskId: string, message: string): Promise<{ task: Task; hash: string }> {
    return this.host.exclusive(taskId, async () => {
      const { hash, subject } = await commitAll(this.host.workspaceOf(taskId), message)
      const task = this.host.tasks.recordGit(taskId, 'git.committed', { hash, subject })
      return { task, hash }
    })
  }

  // A commit message for the changes of the workspace, by the task's agent. It only reads, so a
  // push that runs does not stop it. A second ask while one runs gets the same answer.
  async draftCommitMessage(taskId: string): Promise<CommitMessageDraft> {
    const running = this.drafts.get(taskId)
    if (running !== undefined) {
      return running
    }
    const draft = this.draftNow(taskId).finally(() => this.drafts.delete(taskId))
    this.drafts.set(taskId, draft)
    return draft
  }

  private async draftNow(taskId: string): Promise<CommitMessageDraft> {
    const workspace = this.host.workspaceOf(taskId)
    const task = this.host.tasks.getTask(taskId)
    const agent = task.agent !== null && isLaunchable(task.agent) ? task.agent : 'claude-code'
    const [diff, changes] = await Promise.all([
      readCommitDiff(workspace, MAX_DIFF_CHARS),
      listChanges(workspace),
    ])
    const draft = await draftCommitMessage(
      {
        agent,
        program: this.host.programOf(agent),
        cwd: workspace,
        title: task.title,
        changes,
        diff,
      },
      this.host.runAgentOnce,
    )
    if (draft.source === 'draft') {
      this.host.log(`No commit message draft by ${agent} for ${taskId}: ${draft.reason}`)
    }
    return draft
  }

  // Pushes the task branch to `origin` and makes it the upstream of the branch.
  async push(taskId: string): Promise<Task> {
    return this.host.exclusive(taskId, async () => this.pushNow(taskId))
  }

  // Pushes, then opens a pull request into the default branch. A task that has one keeps it.
  async openPullRequest(taskId: string): Promise<Task> {
    return this.host.exclusive(taskId, async () => {
      const task = this.host.tasks.getTask(taskId)
      if (task.pr !== null) {
        throw new WorkspaceConflictError(`${taskId} has a pull request already: ${task.pr}`)
      }
      const pushed = await this.pushNow(taskId)
      const url = await createPullRequest(
        this.host.workspaceOf(taskId),
        {
          branch: pushed.branch ?? '',
          title: pushed.title,
          body: await this.host.pullRequestBody(pushed),
        },
        this.host.gh,
      )
      return this.host.tasks.recordGit(taskId, 'pr.opened', { url })
    })
  }

  private async pushNow(taskId: string): Promise<Task> {
    const workspace = this.host.workspaceOf(taskId)
    const { branch } = this.host.tasks.getTask(taskId)
    if (branch === null) {
      throw new WorkspaceConflictError(`${taskId} has no branch`)
    }
    await pushBranch(workspace, branch)
    return this.host.tasks.recordGit(taskId, 'git.pushed', { branch })
  }
}
