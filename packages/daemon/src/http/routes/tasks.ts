import {
  attachmentUploadsSchema,
  base64Length,
  GATE_EVENT_TYPES,
  issueLinkSchema,
  MAX_TASK_ATTACHMENT_SIZE,
  previewTypeOf,
  taskIdSchema,
  taskStateSchema,
  templateIdSchema,
} from '@natamux/core'
import type { FastifyInstance } from 'fastify'
import { z } from 'zod'
import { decodeAttachments, listAttachments, readAttachment } from '../../tasks/attachments.js'
import { applyEventInputSchema } from '../../tasks/task-inputs.js'
import { AgentRefusedError } from '../auth.js'
import type { RouteDeps } from '../deps.js'
import { taskRepoBodySchema } from './projects.js'

export const taskParamsSchema = z.object({ id: taskIdSchema })
const listQuerySchema = z.object({ state: taskStateSchema.optional() })
const attachmentsBodySchema = z.object({ attachments: attachmentUploadsSchema.optional() })
// A new task may bring its attachments in its JSON body, as base64.
export const NEW_TASK_BODY_LIMIT = base64Length(MAX_TASK_ATTACHMENT_SIZE) + 1_000_000
const attachmentParamsSchema = z.object({ id: taskIdSchema, name: z.string().min(1).max(255) })

export function registerTaskRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options, byAgent } = deps

  app.get('/tasks', async (request) => {
    const query = listQuerySchema.parse(request.query)
    return { tasks: options.service.listTasks(query.state) }
  })

  app.post('/tasks', { bodyLimit: NEW_TASK_BODY_LIMIT }, async (request, reply) => {
    const { repo } = taskRepoBodySchema.parse(request.body ?? {})
    const { attachments } = attachmentsBodySchema.parse(request.body ?? {})
    options.repos.get(repo ?? null)
    const files = decodeAttachments(attachments ?? [])
    const task = options.service.createTask(request.body, files)
    return reply.code(201).send({ task })
  })

  app.get('/templates', async () => ({ templates: options.templates.list() }))

  app.get('/jobs', async () => ({ jobs: options.jobs?.list() ?? [] }))

  app.post('/templates', async (request, reply) => {
    return reply.code(201).send({ template: options.templates.save(request.body) })
  })

  app.post('/templates/:id/tasks', async (request, reply) => {
    const { id } = z.object({ id: templateIdSchema }).parse(request.params)
    const { repo } = taskRepoBodySchema.parse(request.body ?? {})
    options.repos.get(repo ?? null)
    return reply.code(201).send({ task: options.templates.createTask(id, request.body) })
  })

  app.get('/tasks/:id', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: options.service.getTask(id) }
  })

  // The task, its notes, stage files and history. A workspace goes too. Uncommitted changes
  // refuse the delete, so the task and the worktree both stay.
  app.delete('/tasks/:id', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const current = options.service.getTask(id)
    if (current.workspace !== null) {
      await options.workspaces.remove(id, { deleteBranch: true })
    }
    return { id, tasks: options.service.deleteTask(id) }
  })

  app.patch('/tasks/:id', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: options.service.updateTask(id, request.body) }
  })

  app.get('/tasks/:id/events', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { events: options.service.listEvents(id) }
  })

  app.post('/tasks/:id/events', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const input = applyEventInputSchema.parse(request.body)
    // Any event may end the step before a branch, whose conditions are checked first. Only the
    // daemon picks the path.
    if (!byAgent(request)) {
      return options.service.applyEvent(id, await options.stages.withPath(id, input))
    }
    if (GATE_EVENT_TYPES.includes(input.type)) {
      throw new AgentRefusedError(`send ${input.type}`)
    }
    const event = await options.stages.withPath(id, { ...input, source: 'agent' })
    return options.service.applyEvent(id, event)
  })

  app.get('/tasks/:id/attachments', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    options.service.getTask(id)
    return { attachments: listAttachments(options.dataDir, id) }
  })

  // An image the dashboard shows comes with its type. Any other file, SVG and HTML too, only
  // downloads, so it never runs as a page of the dashboard.
  app.get('/tasks/:id/attachments/:name', async (request, reply) => {
    const { id, name } = attachmentParamsSchema.parse(request.params)
    options.service.getTask(id)
    const bytes = readAttachment(options.dataDir, id, name)
    if (bytes === undefined) {
      return reply.code(404).send({ error: 'not_found', message: `${id} has no ${name}` })
    }
    const type = previewTypeOf(name)
    if (type === null) {
      reply
        .header('content-type', 'application/octet-stream')
        .header('content-disposition', `attachment; filename="${name}"`)
    } else {
      reply.header('content-type', type)
    }
    return reply
      .header('cache-control', 'no-store')
      .header('x-content-type-options', 'nosniff')
      .header('content-security-policy', "default-src 'none'; style-src 'unsafe-inline'; sandbox")
      .send(bytes)
  })

  // Opens a GitHub issue for the task now ("Create issue").
  app.post('/tasks/:id/issue', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    return reply.code(201).send({ task: await options.issueLinker.create(id) })
  })

  app.put('/tasks/:id/issue', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: options.service.linkIssue(id, issueLinkSchema.parse(request.body)) }
  })
}
