import { timingSafeEqual } from 'node:crypto'
import websocket from '@fastify/websocket'
import { TASK_ID_HEADER, taskIdSchema, taskStateSchema } from '@helmsman/core'
import Fastify, { type FastifyInstance } from 'fastify'
import { z } from 'zod'
import { AgentHookService, UnknownAgentError } from './agent-hook-service.js'
import type { EventBus } from './event-bus.js'
import { GitError } from './git.js'
import { TaskNotFoundError, type TaskService } from './task-service.js'
import { WorkspaceConflictError, type WorkspaceService } from './workspace-service.js'

type ServerOptions = {
  service: TaskService
  workspaces: WorkspaceService
  bus: EventBus
  token: string
}

const taskParamsSchema = z.object({ id: taskIdSchema })
const hookParamsSchema = z.object({ agent: z.string().min(1) })
const listQuerySchema = z.object({ state: taskStateSchema.optional() })
const removeWorkspaceQuerySchema = z.object({ deleteBranch: z.enum(['true', 'false']).optional() })

export function isAuthorized(provided: string | undefined, token: string): boolean {
  if (provided === undefined) {
    return false
  }
  const providedBytes = Buffer.from(provided)
  const tokenBytes = Buffer.from(token)
  return providedBytes.length === tokenBytes.length && timingSafeEqual(providedBytes, tokenBytes)
}

function bearerToken(header: string | undefined): string | undefined {
  return /^Bearer (.+)$/.exec(header ?? '')?.[1]
}

function clientErrorStatus(error: unknown): number | undefined {
  if (
    typeof error === 'object' &&
    error !== null &&
    'statusCode' in error &&
    typeof error.statusCode === 'number' &&
    error.statusCode >= 400 &&
    error.statusCode < 500
  ) {
    return error.statusCode
  }
  return undefined
}

export async function buildServer(options: ServerOptions): Promise<FastifyInstance> {
  const app = Fastify({ logger: false })
  const hooks = new AgentHookService(options.service)
  await app.register(websocket)

  // Browsers cannot set headers on a WebSocket, so only /events accepts ?token=.
  app.addHook('onRequest', async (request, reply) => {
    const url = new URL(request.url, 'http://localhost')
    if (url.pathname === '/health') {
      return
    }
    const queryToken = url.pathname === '/events' ? url.searchParams.get('token') : null
    const provided = bearerToken(request.headers.authorization) ?? queryToken ?? undefined
    if (!isAuthorized(provided, options.token)) {
      return reply.code(401).send({ error: 'unauthorized' })
    }
  })

  app.setErrorHandler((error, request, reply) => {
    if (error instanceof z.ZodError) {
      return reply.code(400).send({ error: 'invalid_request', issues: error.issues })
    }
    if (error instanceof TaskNotFoundError) {
      return reply.code(404).send({ error: 'not_found', message: error.message })
    }
    if (error instanceof UnknownAgentError) {
      return reply.code(404).send({ error: 'unknown_agent', message: error.message })
    }
    if (error instanceof WorkspaceConflictError || error instanceof GitError) {
      return reply.code(409).send({ error: 'conflict', message: error.message })
    }
    const status = clientErrorStatus(error)
    if (status !== undefined) {
      const message = error instanceof Error ? error.message : 'Bad request'
      return reply.code(status).send({ error: 'bad_request', message })
    }
    request.log.error(error)
    return reply.code(500).send({ error: 'internal_error' })
  })

  app.get('/health', async () => ({ status: 'ok' }))

  app.get('/tasks', async (request) => {
    const query = listQuerySchema.parse(request.query)
    return { tasks: options.service.listTasks(query.state) }
  })

  app.post('/tasks', async (request, reply) => {
    const task = options.service.createTask(request.body)
    return reply.code(201).send({ task })
  })

  app.get('/tasks/:id', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: options.service.getTask(id) }
  })

  app.get('/tasks/:id/events', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { events: options.service.listEvents(id) }
  })

  app.post('/tasks/:id/events', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return options.service.applyEvent(id, request.body)
  })

  // An agent posts its hook here, with its name in the path. Always answer 200 with an empty object,
  // so a hook that matches no task or changes nothing never shows up as an error in the agent.
  app.post('/hooks/:agent', async (request) => {
    const { agent } = hookParamsSchema.parse(request.params)
    const header = request.headers[TASK_ID_HEADER.toLowerCase()]
    const taskIdHeader = (Array.isArray(header) ? header[0] : header)?.trim() || undefined
    hooks.handle(agent, request.body, taskIdHeader)
    return {}
  })

  app.post('/tasks/:id/workspace', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const task = await options.workspaces.create(id)
    return reply.code(201).send({ task })
  })

  app.delete('/tasks/:id/workspace', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const query = removeWorkspaceQuerySchema.parse(request.query)
    return options.workspaces.remove(id, { deleteBranch: query.deleteBranch === 'true' })
  })

  app.get('/events', { websocket: true }, (socket) => {
    const unsubscribe = options.bus.subscribe((message) => {
      socket.send(JSON.stringify(message))
    })
    socket.on('close', unsubscribe)
    socket.on('error', unsubscribe)
  })

  return app
}
