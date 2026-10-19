import {
  type ChangeEntry,
  type CommitMessageDraft,
  changeEntrySchema,
  commitMessageDraftSchema,
  type FileDiff,
  fileDiffSchema,
  type GitGraph,
  gitGraphSchema,
  type ImageSide,
  type Task,
  taskSchema,
} from '@natamux/core'
import type { Client } from './client'
import { asRecord, stringList } from './parse'
import type { BranchList } from './types'

export function gitApi(client: Client) {
  const { request, requestBlob } = client

  return {
    // `fresh`: the daemon reads git now, not the answer it kept for timed reads.
    async listChanges(taskId: string, fresh = false): Promise<ChangeEntry[]> {
      const path = `/tasks/${encodeURIComponent(taskId)}/changes${fresh ? '?fresh=1' : ''}`
      const body = asRecord(await request('GET', path))
      if (!Array.isArray(body.changes)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.changes.map((entry) => changeEntrySchema.parse(entry))
    },

    async getFileDiff(taskId: string, path: string): Promise<FileDiff> {
      const body = await request(
        'GET',
        `/tasks/${encodeURIComponent(taskId)}/changes/file?path=${encodeURIComponent(path)}`,
      )
      return fileDiffSchema.parse(body)
    },

    // One side of a changed image: the last commit (original) or the file now (modified).
    async getFileImage(taskId: string, path: string, side: ImageSide): Promise<Blob> {
      const query = `path=${encodeURIComponent(path)}&side=${side}`
      return requestBlob(`/tasks/${encodeURIComponent(taskId)}/changes/image?${query}`)
    },

    async getGitGraph(taskId: string, fresh = false): Promise<GitGraph> {
      const path = `/tasks/${encodeURIComponent(taskId)}/graph${fresh ? '?fresh=1' : ''}`
      return gitGraphSchema.parse(await request('GET', path))
    },

    // Commits every change of the task's workspace.
    async commitChanges(taskId: string, message: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/commit`, { message }),
      )
      return taskSchema.parse(body.task)
    },

    // A commit message drafted by the task's agent, or a simple one when the agent failed.
    async generateCommitMessage(taskId: string): Promise<CommitMessageDraft> {
      return commitMessageDraftSchema.parse(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/commit-message`),
      )
    },

    async pushBranch(taskId: string): Promise<Task> {
      const body = asRecord(await request('POST', `/tasks/${encodeURIComponent(taskId)}/push`))
      return taskSchema.parse(body.task)
    },

    // Pushes the branch, then opens a pull request. The task that comes back has its address.
    async openPullRequest(taskId: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/pull-request`),
      )
      return taskSchema.parse(body.task)
    },

    // Asks the daemon to read the pull request of the task from GitHub now.
    async refreshPullRequest(taskId: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/pull-request/refresh`),
      )
      return taskSchema.parse(body.task)
    },

    async listBranches(repo?: string): Promise<BranchList> {
      const query = repo === undefined ? '' : `?repo=${encodeURIComponent(repo)}`
      const body = asRecord(await request('GET', `/git/branches${query}`))
      return {
        branches: stringList(body.branches),
        current: typeof body.current === 'string' ? body.current : null,
        defaultBranch: typeof body.defaultBranch === 'string' ? body.defaultBranch : null,
      }
    },
  }
}
