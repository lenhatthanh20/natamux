import websocket from '@fastify/websocket'
import Fastify, { type FastifyInstance } from 'fastify'
import { AgentHookService } from '../agents/agent-hook-service.js'
import { agentChecks, bearerToken, isAuthorized } from './auth.js'
import type { RouteDeps, ServerOptions } from './deps.js'
import { errorHandler } from './errors.js'
import { registerEventRoutes } from './event-socket.js'
import { registerAgentRoutes } from './routes/agents.js'
import { registerProjectRoutes } from './routes/projects.js'
import { registerStageRoutes } from './routes/stages.js'
import { registerTaskRoutes } from './routes/tasks.js'
import { registerTerminalRoutes } from './routes/terminals.js'
import { registerWorkspaceRoutes } from './routes/workspaces.js'
import { registerWebFallback, stripApiPrefix } from './web-fallback.js'

// GET routes that only read the daemon's memory, files and index, and never run a program, so
// they answer before the login PATH came. Every other route may run git, gh, a shell or an agent.
// A list of the safe ones, so a new route waits until someone checks it.
export const NO_PROGRAM_ROUTES = new Set([
  '/tasks',
  '/tasks/:id',
  '/tasks/:id/events',
  '/tasks/:id/files',
  '/tasks/:id/files/:name',
  '/tasks/:id/tree',
  '/tasks/:id/file',
  '/tasks/:id/attachments',
  '/tasks/:id/attachments/:name',
  '/templates',
  '/activity',
  '/usage',
  '/repos',
  '/repos/:id/settings',
  '/repos/:id/skills',
  '/events',
])

export async function buildServer(options: ServerOptions): Promise<FastifyInstance> {
  const app = Fastify({
    logger: false,
    rewriteUrl: (request) => stripApiPrefix(request.url ?? '/'),
  })
  const hooks = new AgentHookService(options.service, options.stages, {
    bus: options.bus,
    workspaceChanged: (taskId) => options.workspaces.touch(taskId),
    sessionSeen: (taskId, terminal, agent, session) =>
      options.openShells?.noteSession(taskId, terminal, agent, session),
    usageSeen: (taskId, stage, agent, transcript) =>
      options.usage?.seen(taskId, stage, agent, transcript),
  })
  await app.register(websocket)
  const servesPage =
    options.webIndexPath === undefined
      ? null
      : registerWebFallback(app, options.webIndexPath, options.webDir)

  // Browsers cannot set headers on a WebSocket, so only /events accepts ?token=. The terminal
  // socket checks a one-time ticket itself instead (see TerminalService.issueTicket).
  // Decide by the route that matched, never by the path parsed again: new URL reads "\" as "/"
  // and resolves "..", the router does not.
  app.addHook('onRequest', async (request, reply) => {
    const route = request.routeOptions.url
    if (route === '/health' || servesPage?.(request) === true) {
      return
    }
    if (request.method !== 'GET' || !NO_PROGRAM_ROUTES.has(route ?? '')) {
      await options.ready
    }
    const query = new URL(request.url, 'http://localhost').searchParams
    if (request.method === 'GET' && route === '/tasks/:id/terminal' && query.has('ticket')) {
      return
    }
    const queryToken = route === '/events' ? query.get('token') : null
    const provided = bearerToken(request.headers.authorization) ?? queryToken ?? undefined
    if (!isAuthorized(provided, options.token) && !isAuthorized(provided, options.agentToken)) {
      return reply.code(401).send({ error: 'unauthorized' })
    }
  })
  const { byAgent, refuseAgent } = agentChecks(options.agentToken)

  app.setErrorHandler(errorHandler)

  app.get('/health', async () => ({ status: 'ok' }))

  const deps: RouteDeps = { options, hooks, byAgent, refuseAgent }
  registerTaskRoutes(app, deps)
  registerStageRoutes(app, deps)
  registerWorkspaceRoutes(app, deps)
  registerAgentRoutes(app, deps)
  registerProjectRoutes(app, deps)
  registerTerminalRoutes(app, deps)
  registerEventRoutes(app, deps)

  return app
}
