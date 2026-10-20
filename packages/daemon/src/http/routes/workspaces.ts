import {
  attachmentUploadsSchema,
  IMAGE_SIDES,
  repoIdSchema,
  taskContextSchema,
  taskSchema,
} from '@natamux/core'
import type { FastifyInstance } from 'fastify'
import { z } from 'zod'
import { LAUNCHABLE_AGENTS } from '../../agents/agent-runner.js'
import { decodeAttachments } from '../../tasks/attachments.js'
import { fanOut, MAX_TRIES } from '../../workspaces/fan-out.js'
import { newWorkspace } from '../../workspaces/new-workspace.js'
import { singleHeader } from '../auth.js'
import type { RouteDeps } from '../deps.js'
import { NEW_TASK_BODY_LIMIT, taskParamsSchema } from './tasks.js'

const removeWorkspaceQuerySchema = z.object({ deleteBranch: z.enum(['true', 'false']).optional() })
const changedFileQuerySchema = z.object({ path: z.string().min(1).max(1024) })
// A manual refresh: git is read now, not the answer kept from a timed read.
const freshQuerySchema = z.object({ fresh: z.enum(['1']).optional() })
const changedImageQuerySchema = changedFileQuerySchema.extend({ side: z.enum(IMAGE_SIDES) })
// The worktree root is the empty path.
const treeQuerySchema = z.object({ path: z.string().max(1024).default('') })
const workspaceFileBodySchema = z.object({ text: z.string() })
// A text of 1 MB, sent as JSON: escapes can make it a few times longer.
const WORKSPACE_FILE_BODY_LIMIT = 4 * 1024 * 1024
const fanOutBodySchema = z.object({
  count: z.number().int().min(1).max(MAX_TRIES),
  start: z.boolean().default(false),
})
const newWorkspaceBodySchema = z.object({
  title: taskSchema.shape.title,
  // Null, like none, is the default repository.
  repo: repoIdSchema.nullish().transform((repo) => repo ?? undefined),
  base: z.string().min(1).max(255).optional(),
  // The first stage starts at once, so it must be an agent the daemon can start.
  agent: z.enum(LAUNCHABLE_AGENTS).optional(),
  prompt: z.string().trim().max(20_000).optional(),
  context: taskContextSchema.optional(),
  attachments: attachmentUploadsSchema.optional(),
})
const createWorkspaceBodySchema = z
  .object({ base: z.string().min(1).max(255).optional() })
  .optional()
const commitBodySchema = z.object({ message: z.string().trim().min(1).max(10_000) })
const rollbackBodySchema = z.object({
  paths: z.array(changedFileQuerySchema.shape.path).min(1).max(5000),
})

export function registerWorkspaceRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options } = deps

  app.get('/diffstats', async () => ({ stats: await options.workspaces.diffStats() }))

  app.post('/tasks/:id/workspace', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const body = createWorkspaceBodySchema.parse(request.body)
    const task = await options.workspaces.create(id, body?.base)
    return reply.code(201).send({ task })
  })

  app.delete('/tasks/:id/workspace', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const query = removeWorkspaceQuerySchema.parse(request.query)
    return options.workspaces.remove(id, { deleteBranch: query.deleteBranch === 'true' })
  })

  app.get('/tasks/:id/changes', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { fresh } = freshQuerySchema.parse(request.query)
    return { changes: await options.workspaces.changes(id, fresh === '1') }
  })

  app.get('/tasks/:id/changes/file', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { path } = changedFileQuerySchema.parse(request.query)
    return options.workspaces.fileDiff(id, path)
  })

  // The bytes of one side of a changed image. An SVG can hold scripts, so the answer may not run
  // anything, even when it is opened on its own.
  app.get('/tasks/:id/changes/image', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { path, side } = changedImageQuerySchema.parse(request.query)
    const { type, bytes } = await options.workspaces.fileImage(id, path, side)
    return reply
      .header('content-type', type)
      .header('cache-control', 'no-store')
      .header('x-content-type-options', 'nosniff')
      .header('content-security-policy', "default-src 'none'; style-src 'unsafe-inline'; sandbox")
      .send(bytes)
  })

  // The files explorer. One folder per request, so a big worktree is read only where it is opened.
  app.get('/tasks/:id/tree', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { path } = treeQuerySchema.parse(request.query)
    return { entries: await options.workspaces.tree(id, path) }
  })

  app.get('/tasks/:id/file', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { path } = changedFileQuerySchema.parse(request.query)
    return options.workspaces.readFile(id, path)
  })

  // `If-Match` holds the version the editor read; a file that changed since is not replaced.
  app.put('/tasks/:id/file', { bodyLimit: WORKSPACE_FILE_BODY_LIMIT }, async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { path } = changedFileQuerySchema.parse(request.query)
    const { text } = workspaceFileBodySchema.parse(request.body)
    const version = singleHeader(request.headers['if-match'])
    if (version === undefined) {
      return reply.code(400).send({ error: 'invalid_request', message: 'If-Match is missing' })
    }
    return options.workspaces.writeFile(id, path, text, version)
  })

  app.get('/tasks/:id/graph', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { fresh } = freshQuerySchema.parse(request.query)
    return options.workspaces.graph(id, fresh === '1')
  })

  app.post('/tasks/:id/fan-out', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { count, start } = fanOutBodySchema.parse(request.body)
    const parts = {
      tasks: options.service,
      workspaces: options.workspaces,
      stages: options.stages,
      dataDir: options.dataDir,
    }
    return reply.code(201).send({ tasks: await fanOut(parts, id, { count, start }) })
  })

  // A task, its worktree and its agent in one step. The answer comes before the worktree is made.
  app.post('/workspaces', { bodyLimit: NEW_TASK_BODY_LIMIT }, async (request, reply) => {
    const { attachments, ...input } = newWorkspaceBodySchema.parse(request.body)
    // Before the task is made, so a missing repository or a file over a limit leaves no task.
    options.repos.get(input.repo ?? null)
    const files = decodeAttachments(attachments ?? [])
    const parts = {
      tasks: options.service,
      workspaces: options.workspaces,
      agents: options.agents,
      stages: options.stages,
    }
    const task = await newWorkspace(parts, { ...input, attachments: files })
    return reply.code(202).send({ task })
  })

  app.get('/workspaces/preparing', async () => ({ preparing: options.workspaces.preparing() }))

  app.post('/tasks/:id/commit', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { message } = commitBodySchema.parse(request.body)
    return options.workspaces.commit(id, message)
  })

  // Changed files back as they are in the last commit. Only the last one can be undone.
  app.post('/tasks/:id/rollback', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { paths } = rollbackBodySchema.parse(request.body)
    return options.workspaces.rollback(id, paths)
  })

  app.post('/tasks/:id/rollback/undo', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return options.workspaces.undoRollback(id)
  })

  // A draft of the commit message, for the user to read before the commit.
  app.post('/tasks/:id/commit-message', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return options.workspaces.draftCommitMessage(id)
  })

  app.post('/tasks/:id/push', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: await options.workspaces.push(id) }
  })

  app.post('/tasks/:id/pull-request', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: await options.workspaces.openPullRequest(id) }
  })

  // Asks GitHub now instead of waiting for the next minute.
  app.post('/tasks/:id/pull-request/refresh', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: await options.pullRequests.check(id) }
  })
}
