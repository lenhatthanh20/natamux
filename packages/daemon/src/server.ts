import { timingSafeEqual } from 'node:crypto'
import websocket from '@fastify/websocket'
import {
  TASK_ID_HEADER,
  type TerminalServerMessage,
  taskIdSchema,
  taskStateSchema,
  terminalClientMessageSchema,
  terminalSizeSchema,
} from '@helmsman/core'
import Fastify, { type FastifyInstance } from 'fastify'
import { z } from 'zod'
import { AgentHookService, UnknownAgentError } from './agent-hook-service.js'
import type { AgentRunner } from './agent-runner.js'
import type { EventBus } from './event-bus.js'
import { GitError, GitHubError, NothingToCommitError, OutsideWorkspaceError } from './git.js'
import { TaskNotFoundError, type TaskService } from './task-service.js'
import { TerminalLimitError, type TerminalService } from './terminal-service.js'
import {
  FileNotChangedError,
  WorkspaceConflictError,
  type WorkspaceService,
} from './workspace-service.js'

type ServerOptions = {
  service: TaskService
  workspaces: WorkspaceService
  terminals: TerminalService
  agents: AgentRunner
  bus: EventBus
  token: string
}

const taskParamsSchema = z.object({ id: taskIdSchema })
const hookParamsSchema = z.object({ agent: z.string().min(1) })
const listQuerySchema = z.object({ state: taskStateSchema.optional() })
const removeWorkspaceQuerySchema = z.object({ deleteBranch: z.enum(['true', 'false']).optional() })
const changedFileQuerySchema = z.object({ path: z.string().min(1).max(1024) })
const startAgentBodySchema = z.object({
  agent: z.string().min(1).max(64),
  prompt: z.string().max(20_000).optional(),
})
const commitBodySchema = z.object({ message: z.string().trim().min(1).max(10_000) })
const terminalQuerySchema = terminalSizeSchema.extend({ ticket: z.string().min(1).max(256) })
const TERMINAL_PATH = /^\/tasks\/[^/]+\/terminal$/

// Close codes of the terminal socket, in the range that applications may use.
const CLOSE_UNAUTHORIZED = 4401
const CLOSE_REFUSED = 4409

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

  // Browsers cannot set headers on a WebSocket, so only /events accepts ?token=. The terminal
  // socket checks a one-time ticket itself instead (see TerminalService.issueTicket).
  app.addHook('onRequest', async (request, reply) => {
    const url = new URL(request.url, 'http://localhost')
    if (url.pathname === '/health') {
      return
    }
    if (
      request.method === 'GET' &&
      TERMINAL_PATH.test(url.pathname) &&
      url.searchParams.has('ticket')
    ) {
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
    if (error instanceof FileNotChangedError) {
      return reply.code(404).send({ error: 'not_found', message: error.message })
    }
    if (
      error instanceof WorkspaceConflictError ||
      error instanceof GitError ||
      error instanceof OutsideWorkspaceError ||
      error instanceof TerminalLimitError ||
      error instanceof NothingToCommitError ||
      error instanceof GitHubError
    ) {
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
    // A shell that runs in the folder would keep files open while git removes it.
    options.terminals.close(id)
    return options.workspaces.remove(id, { deleteBranch: query.deleteBranch === 'true' })
  })

  app.get('/tasks/:id/changes', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { changes: await options.workspaces.changes(id) }
  })

  app.get('/tasks/:id/changes/file', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { path } = changedFileQuerySchema.parse(request.query)
    return options.workspaces.fileDiff(id, path)
  })

  app.get('/tasks/:id/graph', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return options.workspaces.graph(id)
  })

  app.post('/tasks/:id/agent', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { agent, prompt } = startAgentBodySchema.parse(request.body)
    return { task: options.agents.start(id, agent, prompt) }
  })

  app.post('/tasks/:id/commit', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { message } = commitBodySchema.parse(request.body)
    return options.workspaces.commit(id, message)
  })

  app.post('/tasks/:id/push', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: await options.workspaces.push(id) }
  })

  app.post('/tasks/:id/pull-request', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: await options.workspaces.openPullRequest(id) }
  })

  app.post('/tasks/:id/terminal/ticket', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { ticket: options.terminals.issueTicket(id) }
  })

  // A shell in the task's workspace. Frames are JSON, see terminal.ts in @helmsman/core.
  app.get('/tasks/:id/terminal', { websocket: true }, (socket, request) => {
    const params = taskParamsSchema.safeParse(request.params)
    const query = terminalQuerySchema.safeParse(request.query)
    if (
      !params.success ||
      !query.success ||
      !options.terminals.redeemTicket(query.data.ticket, params.data.id)
    ) {
      socket.close(CLOSE_UNAUTHORIZED, 'unauthorized')
      return
    }
    const taskId = params.data.id
    const send = (message: TerminalServerMessage) => {
      socket.send(JSON.stringify(message))
      if (message.type === 'exit') {
        socket.close()
      }
    }

    let detach: () => void
    try {
      detach = options.terminals.attach(taskId, { send }, query.data)
    } catch (error) {
      const message = error instanceof Error ? error.message : 'The terminal could not start.'
      socket.send(JSON.stringify({ type: 'error', message } satisfies TerminalServerMessage))
      socket.close(CLOSE_REFUSED, 'refused')
      return
    }

    socket.on('message', (raw: { toString: () => string }) => {
      let data: unknown
      try {
        data = JSON.parse(raw.toString())
      } catch {
        return
      }
      const message = terminalClientMessageSchema.safeParse(data)
      if (!message.success) {
        return
      }
      if (message.data.type === 'input') {
        options.terminals.input(taskId, message.data.data)
      } else {
        options.terminals.resize(taskId, message.data)
      }
    })
    socket.on('close', () => detach())
    socket.on('error', () => detach())
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
