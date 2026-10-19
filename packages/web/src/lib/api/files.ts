import {
  type Attachment,
  attachmentSchema,
  type TreeEntry,
  treeEntrySchema,
  type WorkspaceFile,
  workspaceFileSchema,
} from '@natamux/core'
import type { Client } from './client'
import { asRecord } from './parse'

export function filesApi(client: Client) {
  const { request, requestBlob } = client

  return {
    // One folder of the worktree, for the files explorer. '' is the root.
    async listFolder(taskId: string, path: string): Promise<TreeEntry[]> {
      const url = `/tasks/${encodeURIComponent(taskId)}/tree?path=${encodeURIComponent(path)}`
      const body = asRecord(await request('GET', url))
      if (!Array.isArray(body.entries)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.entries.map((entry) => treeEntrySchema.parse(entry))
    },

    async readWorkspaceFile(taskId: string, path: string): Promise<WorkspaceFile> {
      const id = encodeURIComponent(taskId)
      const body = await request('GET', `/tasks/${id}/file?path=${encodeURIComponent(path)}`)
      return workspaceFileSchema.parse(body)
    },

    // Answers the new version. The daemon answers 409 (see isConflict) when the file changed
    // after `version` was read.
    async writeWorkspaceFile(
      taskId: string,
      path: string,
      text: string,
      version: string,
    ): Promise<string> {
      const id = encodeURIComponent(taskId)
      const body = asRecord(
        await request(
          'PUT',
          `/tasks/${id}/file?path=${encodeURIComponent(path)}`,
          { text },
          { 'if-match': version },
        ),
      )
      return String(body.version ?? '')
    },

    async listAttachments(taskId: string): Promise<Attachment[]> {
      const path = `/tasks/${encodeURIComponent(taskId)}/attachments`
      const body = asRecord(await request('GET', path))
      if (!Array.isArray(body.attachments)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.attachments.map((entry) => attachmentSchema.parse(entry))
    },

    async readAttachment(taskId: string, name: string): Promise<Blob> {
      const id = encodeURIComponent(taskId)
      return requestBlob(`/tasks/${id}/attachments/${encodeURIComponent(name)}`)
    },
  }
}
