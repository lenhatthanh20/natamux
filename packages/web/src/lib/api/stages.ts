import { type Task, taskSchema } from '@natamux/core'
import type { Client } from './client'
import { asRecord } from './parse'
import type { StageAction, StageActionInput, StageFile } from './types'

export function stagesApi(client: Client) {
  const { request } = client

  return {
    async stageAction(
      taskId: string,
      action: StageAction,
      input: StageActionInput = {},
    ): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/stage`, { action, ...input }),
      )
      return taskSchema.parse(body.task)
    },

    async setCi(taskId: string, stage: string, on: boolean): Promise<Task> {
      const body = asRecord(
        await request('PUT', `/tasks/${encodeURIComponent(taskId)}/ci`, { stage, on }),
      )
      return taskSchema.parse(body.task)
    },

    async setSkip(taskId: string, stage: string, on: boolean): Promise<Task> {
      const body = asRecord(
        await request('PUT', `/tasks/${encodeURIComponent(taskId)}/skip`, { stage, on }),
      )
      return taskSchema.parse(body.task)
    },

    async setWorkflow(taskId: string, stage: string, on: boolean): Promise<Task> {
      const body = asRecord(
        await request('PUT', `/tasks/${encodeURIComponent(taskId)}/workflow`, { stage, on }),
      )
      return taskSchema.parse(body.task)
    },

    async listStageFiles(taskId: string): Promise<StageFile[]> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/files`))
      if (!Array.isArray(body.files)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.files.map((entry) => {
        const file = asRecord(entry)
        return {
          name: String(file.name),
          size: Number(file.size),
          modifiedAt: String(file.modifiedAt),
        }
      })
    },

    async readStageFile(taskId: string, name: string): Promise<string> {
      const body = asRecord(
        await request(
          'GET',
          `/tasks/${encodeURIComponent(taskId)}/files/${encodeURIComponent(name)}`,
        ),
      )
      return String(body.text ?? '')
    },

    async writeStageFile(taskId: string, name: string, text: string): Promise<void> {
      await request(
        'PUT',
        `/tasks/${encodeURIComponent(taskId)}/files/${encodeURIComponent(name)}`,
        {
          text,
        },
      )
    },
  }
}
