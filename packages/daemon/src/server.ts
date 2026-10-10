import { timingSafeEqual } from 'node:crypto'
import websocket from '@fastify/websocket'
import {
  IMAGE_SIDES,
  issueLinkSchema,
  repoIdSchema,
  TASK_ID_HEADER,
  TERMINAL_HEADER,
  type TerminalServerMessage,
  taskContextSchema,
  taskIdSchema,
  taskStateSchema,
  templateIdSchema,
  terminalClientMessageSchema,
  terminalSizeSchema,
} from '@natamux/core'
import Fastify, { type FastifyInstance } from 'fastify'
import { z } from 'zod'
import { AgentHookService, UnknownAgentError } from './agent-hook-service.js'
import { type AgentRunner, LAUNCHABLE_AGENTS, TaskWaitingError } from './agent-runner.js'
import { COPILOT_EVENT_FIELD } from './copilot-hooks.js'
import type { EventBus } from './event-bus.js'
import { fanOut, MAX_TRIES } from './fan-out.js'
import {
  GitError,
  GitHubError,
  ImageSideMissingError,
  ImageTooLargeError,
  NotAnImageError,
  NothingToCommitError,
  OutsideWorkspaceError,
} from './git.js'
import type { IssueImporter } from './issue-importer.js'
import { IssueLinkError, type IssueLinker } from './issue-linker.js'
import { newWorkspace } from './new-workspace.js'
import { type ProjectSettingsService, SettingsInputError } from './project-settings.js'
import type { PullRequestWatcher } from './pull-request-watcher.js'
import { RepoInputError, RepoNotFoundError, type RepoService } from './repo-service.js'
import {
  archiveStageFiles,
  listStageFiles,
  readStageFile,
  StageFileError,
  writeStageFile,
} from './stage-files.js'
import {
  STAGE_ACTIONS,
  StageActionError,
  StageInputError,
  type StageRunner,
} from './stage-runner.js'
import { TaskEditError, TaskNotFoundError, type TaskService } from './task-service.js'
import { TemplateNotFoundError, type TemplateService } from './template-service.js'
import {
  MAIN_TERMINAL,
  NoTerminalError,
  TerminalLimitError,
  type TerminalService,
} from './terminal-service.js'
import { registerWebFallback, stripApiPrefix } from './web-fallback.js'
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
  pullRequests: PullRequestWatcher
  issues: IssueImporter
  templates: TemplateService
  repos: RepoService
  bus: EventBus
  stages: StageRunner
  settings: ProjectSettingsService
  issueLinker: IssueLinker
  // The daemon's data folder, where stage files are kept for tasks without a worktree.
  dataDir: string
  token: string
  // The built dashboard's index.html. With it, an address of the dashboard that is not an API
  // route (a deep link) answers with that page when the browser asks for HTML.
  webIndexPath?: string
  // The built dashboard. Set: the daemon serves its files too, at the same address as the API.
  webDir?: string
}

const taskParamsSchema = z.object({ id: taskIdSchema })
const hookParamsSchema = z.object({ agent: z.string().min(1) })
const hookQuerySchema = z.object({ event: z.string().min(1).max(64).optional() })
const listQuerySchema = z.object({ state: taskStateSchema.optional() })
const removeWorkspaceQuerySchema = z.object({ deleteBranch: z.enum(['true', 'false']).optional() })
const changedFileQuerySchema = z.object({ path: z.string().min(1).max(1024) })
const changedImageQuerySchema = changedFileQuerySchema.extend({ side: z.enum(IMAGE_SIDES) })
const fanOutBodySchema = z.object({
  count: z.number().int().min(1).max(MAX_TRIES),
  start: z.boolean().default(false),
})
const messageBodySchema = z.object({ text: z.string().trim().min(1).max(20_000) })
const newWorkspaceBodySchema = z.object({
  title: z.string().trim().min(1).max(200),
  repo: repoIdSchema.optional(),
  base: z.string().min(1).max(255).optional(),
  // The first stage starts at once, so it must be an agent the daemon can start.
  agent: z.enum(LAUNCHABLE_AGENTS).optional(),
  prompt: z.string().trim().max(20_000).optional(),
  context: taskContextSchema.optional(),
})
const startAgentBodySchema = z.object({
  agent: z.string().min(1).max(64),
  prompt: z.string().max(20_000).optional(),
})
const importIssuesBodySchema = z.object({
  numbers: z.array(z.number().int().positive()).min(1).max(100),
  repo: repoIdSchema.optional(),
})
const repoQuerySchema = z.object({ repo: repoIdSchema.optional() })
const createWorkspaceBodySchema = z
  .object({ base: z.string().min(1).max(255).optional() })
  .optional()
const commitBodySchema = z.object({ message: z.string().trim().min(1).max(10_000) })
const stageBodySchema = z.object({
  action: z.enum(STAGE_ACTIONS),
  stage: z.string().min(1).max(40).optional(),
  terminal: z
    .string()
    .regex(/^[1-9]\d{0,2}$/)
    .optional(),
  to: z.string().min(1).max(40).optional(),
  reason: z.string().trim().min(1).max(20_000).optional(),
})
const stageFileParamsSchema = z.object({ id: taskIdSchema, name: z.string().min(1).max(100) })
const stageFileBodySchema = z.object({ text: z.string() })
const repoParamsSchema = z.object({ id: repoIdSchema })
const terminalNameSchema = z.string().regex(/^[1-9]\d{0,2}$/, 'A terminal is named by a number')
const terminalQuerySchema = terminalSizeSchema.extend({
  ticket: z.string().min(1).max(256),
  terminal: terminalNameSchema.default(MAIN_TERMINAL),
})
const terminalParamsSchema = z.object({ id: taskIdSchema, terminal: terminalNameSchema })
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
  const app = Fastify({
    logger: false,
    rewriteUrl: (request) => stripApiPrefix(request.url ?? '/'),
  })
  const hooks = new AgentHookService(options.service, options.stages)
  await app.register(websocket)
  const servesPage =
    options.webIndexPath === undefined
      ? null
      : registerWebFallback(app, options.webIndexPath, options.webDir)

  // Browsers cannot set headers on a WebSocket, so only /events accepts ?token=. The terminal
  // socket checks a one-time ticket itself instead (see TerminalService.issueTicket).
  app.addHook('onRequest', async (request, reply) => {
    const url = new URL(request.url, 'http://localhost')
    if (url.pathname === '/health' || servesPage?.(request) === true) {
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
    if (error instanceof TaskEditError) {
      return reply.code(400).send({ error: 'invalid_request', message: error.message })
    }
    if (error instanceof TaskNotFoundError) {
      return reply.code(404).send({ error: 'not_found', message: error.message })
    }
    if (error instanceof UnknownAgentError) {
      return reply.code(404).send({ error: 'unknown_agent', message: error.message })
    }
    if (
      error instanceof RepoInputError ||
      error instanceof SettingsInputError ||
      error instanceof StageFileError ||
      error instanceof StageInputError
    ) {
      return reply.code(400).send({ error: 'invalid_request', message: error.message })
    }
    if (error instanceof TemplateNotFoundError || error instanceof RepoNotFoundError) {
      return reply.code(404).send({ error: 'not_found', message: error.message })
    }
    if (error instanceof FileNotChangedError || error instanceof ImageSideMissingError) {
      return reply.code(404).send({ error: 'not_found', message: error.message })
    }
    if (error instanceof NotAnImageError) {
      return reply.code(400).send({ error: 'invalid_request', message: error.message })
    }
    if (error instanceof ImageTooLargeError) {
      return reply.code(413).send({ error: 'too_large', message: error.message })
    }
    if (
      error instanceof WorkspaceConflictError ||
      error instanceof GitError ||
      error instanceof OutsideWorkspaceError ||
      error instanceof TerminalLimitError ||
      error instanceof NoTerminalError ||
      error instanceof NothingToCommitError ||
      error instanceof GitHubError ||
      error instanceof TaskWaitingError ||
      error instanceof StageActionError ||
      error instanceof IssueLinkError
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
    const { repo } = repoQuerySchema.parse(request.body ?? {})
    if (repo !== undefined) {
      options.repos.get(repo)
    }
    const task = options.service.createTask(request.body)
    return reply.code(201).send({ task })
  })

  app.get('/templates', async () => ({ templates: options.templates.list() }))

  app.post('/templates', async (request, reply) => {
    return reply.code(201).send({ template: options.templates.save(request.body) })
  })

  app.post('/templates/:id/tasks', async (request, reply) => {
    const { id } = z.object({ id: templateIdSchema }).parse(request.params)
    return reply.code(201).send({ task: options.templates.createTask(id, request.body) })
  })

  // What the agent of each task does now, from its last hook.
  app.get('/activity', async () => ({ activity: hooks.activity() }))

  app.get('/repos', async () => ({ repos: options.repos.list() }))

  app.post('/repos', async (request, reply) => {
    return reply.code(201).send({ repo: await options.repos.add(request.body) })
  })

  app.get('/git/branches', async (request) => {
    const { repo } = repoQuerySchema.parse(request.query)
    return options.workspaces.branches(repo ?? null)
  })

  app.get('/diffstats', async () => ({ stats: await options.workspaces.diffStats() }))

  app.get('/github/issues', async (request) => {
    const { repo } = repoQuerySchema.parse(request.query)
    return { issues: await options.issues.list(repo ?? null) }
  })

  app.post('/github/issues/import', async (request) => {
    const { numbers, repo } = importIssuesBodySchema.parse(request.body)
    return { tasks: await options.issues.import(numbers, repo ?? null) }
  })

  app.get('/tasks/:id', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: options.service.getTask(id) }
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
    return options.service.applyEvent(id, request.body)
  })

  // An agent posts its hook here, with its name in the path. Always answer 200 with an empty object,
  // so a hook that matches no task or changes nothing never shows up as an error in the agent.
  app.post('/hooks/:agent', async (request) => {
    const { agent } = hookParamsSchema.parse(request.params)
    const header = (name: string) => {
      const value = request.headers[name.toLowerCase()]
      return (Array.isArray(value) ? value[0] : value)?.trim() || undefined
    }
    // Copilot CLI does not name the event in the JSON; its hook command adds it as ?event=.
    const { event } = hookQuerySchema.parse(request.query)
    const body =
      event !== undefined && typeof request.body === 'object' && request.body !== null
        ? { ...request.body, [COPILOT_EVENT_FIELD]: event }
        : request.body
    hooks.handle(agent, body, header(TASK_ID_HEADER), header(TERMINAL_HEADER))
    return {}
  })

  // Moves a task through its stages: approve, send back, skip, pass, fail, report done, start.
  app.post('/tasks/:id/stage', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { action, ...input } = stageBodySchema.parse(request.body)
    return { task: await options.stages.act(id, action, input) }
  })

  app.get('/tasks/:id/files', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { files: listStageFiles(options.dataDir, options.service.getTask(id)) }
  })

  app.get('/tasks/:id/files/:name', async (request, reply) => {
    const { id, name } = stageFileParamsSchema.parse(request.params)
    const text = readStageFile(options.dataDir, options.service.getTask(id), name)
    if (text === undefined) {
      return reply.code(404).send({ error: 'not_found', message: `${id} has no ${name}` })
    }
    return { name, text }
  })

  // An edit by hand, for example to fix the spec before approving it.
  app.put('/tasks/:id/files/:name', async (request) => {
    const { id, name } = stageFileParamsSchema.parse(request.params)
    const { text } = stageFileBodySchema.parse(request.body)
    writeStageFile(options.dataDir, options.service.getTask(id), name, text)
    return { name, text }
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

  app.get('/repos/:id/settings', async (request) => {
    const { id } = repoParamsSchema.parse(request.params)
    const loaded = options.settings.load(id)
    return { ...loaded, path: options.settings.pathOf(id) }
  })

  app.put('/repos/:id/settings', async (request) => {
    const { id } = repoParamsSchema.parse(request.params)
    return { settings: options.settings.save(id, request.body) }
  })

  app.post('/tasks/:id/workspace', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const body = createWorkspaceBodySchema.parse(request.body)
    const task = await options.workspaces.create(id, body?.base)
    return reply.code(201).send({ task })
  })

  app.delete('/tasks/:id/workspace', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const query = removeWorkspaceQuerySchema.parse(request.query)
    archiveStageFiles(options.dataDir, options.service.getTask(id))
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

  app.get('/tasks/:id/graph', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return options.workspaces.graph(id)
  })

  app.post('/tasks/:id/agent', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { agent, prompt } = startAgentBodySchema.parse(request.body)
    return { task: await options.stages.startByHand(id, agent, prompt) }
  })

  app.post('/tasks/:id/fan-out', async (request, reply) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { count, start } = fanOutBodySchema.parse(request.body)
    const parts = { tasks: options.service, workspaces: options.workspaces, stages: options.stages }
    return reply.code(201).send({ tasks: await fanOut(parts, id, { count, start }) })
  })

  // A task, its worktree and its agent in one step.
  app.post('/workspaces', async (request, reply) => {
    const input = newWorkspaceBodySchema.parse(request.body)
    const parts = {
      tasks: options.service,
      workspaces: options.workspaces,
      agents: options.agents,
      stages: options.stages,
    }
    return reply.code(201).send({ task: await newWorkspace(parts, input) })
  })

  // A review comment for the agent: pasted into its terminal as the next prompt.
  app.post('/tasks/:id/agent/message', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { text } = messageBodySchema.parse(request.body)
    options.service.getTask(id)
    // Pasted into a shell, the lines of the comment would run as commands.
    if (!options.stages.agentRuns(id, MAIN_TERMINAL)) {
      throw new StageActionError(`No agent runs in the terminal of ${id}. Start the agent first`)
    }
    options.terminals.paste(id, text)
    return { task: options.service.recordReviewComment(id, text) }
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

  // Asks GitHub now instead of waiting for the next minute.
  app.post('/tasks/:id/pull-request/refresh', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: await options.pullRequests.check(id) }
  })

  app.get('/tasks/:id/terminals', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    options.service.getTask(id)
    const terminals = options.terminals.list(id)
    // The shells where an agent runs now, so a shell left after a restart is not called the agent's.
    const agents = terminals.filter((name) => options.stages.agentRuns(id, name))
    return { terminals, agents }
  })

  app.delete('/tasks/:id/terminals/:terminal', async (request) => {
    const { id, terminal } = terminalParamsSchema.parse(request.params)
    options.terminals.closeOne(id, terminal)
    return { terminals: options.terminals.list(id) }
  })

  app.post('/tasks/:id/terminal/ticket', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { ticket: options.terminals.issueTicket(id) }
  })

  // A shell in the task's workspace. Frames are JSON, see terminal.ts in @natamux/core.
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
    const { terminal } = query.data
    const send = (message: TerminalServerMessage) => {
      socket.send(JSON.stringify(message))
      if (message.type === 'exit') {
        socket.close()
      }
    }

    let detach: () => void
    try {
      detach = options.terminals.attach(taskId, { send }, query.data, terminal)
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
        options.terminals.input(taskId, message.data.data, terminal)
      } else {
        options.terminals.resize(taskId, message.data, terminal)
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
