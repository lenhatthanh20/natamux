import { basename } from 'node:path'
import {
  CLOSE_GONE,
  CLOSE_REFUSED,
  CLOSE_UNAUTHORIZED,
  SILENT_CLOSE_MS,
  shellIdSchema,
  type TerminalServerMessage,
  taskIdSchema,
  terminalClientMessageSchema,
  terminalSizeSchema,
} from '@natamux/core'
import type { FastifyInstance, FastifyRequest } from 'fastify'
import { z } from 'zod'
import { readEndedShells } from '../../terminals/ended-shells.js'
import { MAIN_TERMINAL } from '../../terminals/terminal-limits.js'
import { ShellGoneError, type TerminalClient } from '../../terminals/terminal-types.js'
import type { RouteDeps } from '../deps.js'
import { taskParamsSchema } from './tasks.js'

const terminalNameSchema = z.string().regex(/^[1-9]\d{0,2}$/, 'A terminal is named by a number')
const terminalQuerySchema = terminalSizeSchema.extend({
  ticket: z.string().min(1).max(256),
  terminal: terminalNameSchema.default(MAIN_TERMINAL),
  // Whether the terminal is on screen when it connects. Only then it takes the shell's size.
  visible: z.enum(['0', '1']).default('1'),
  // A client that lost its socket resumes its shell, from `offset` when the ring still has it.
  shellId: shellIdSchema.optional(),
  offset: z.coerce.number().int().min(0).optional(),
})
const terminalParamsSchema = z.object({ id: taskIdSchema, terminal: terminalNameSchema })

// The part of a WebSocket that the terminal socket uses.
type TerminalSocket = {
  send(data: Uint8Array | string): void
  close(code?: number, reason?: string): void
  terminate(): void
  on(event: 'message' | 'close' | 'error', listener: (...args: never[]) => void): unknown
}

export function registerTerminalRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options } = deps

  // The last screens of the shells of a done or cancelled task, for its wrap-up.
  app.get('/tasks/:id/ended-shells', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    options.service.getTask(id)
    return { shells: readEndedShells(options.dataDir, id) }
  })

  app.get('/tasks/:id/terminals', (request) => listTerminals(deps, request))

  app.delete('/tasks/:id/terminals/:terminal', async (request) => {
    const { id, terminal } = terminalParamsSchema.parse(request.params)
    options.terminals.closeOne(id, terminal)
    return { terminals: options.terminals.list(id) }
  })

  app.post('/tasks/:id/terminal/ticket', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { ticket: options.terminals.issueTicket(id) }
  })

  // A shell in the task's workspace. Output is binary, control messages are JSON, see terminal.ts
  // in @natamux/core.
  app.get('/tasks/:id/terminal', { websocket: true }, (socket, request) =>
    handleTerminalSocket(deps, socket, request),
  )
}

async function listTerminals({ options, hooks }: RouteDeps, request: FastifyRequest) {
  const { id } = taskParamsSchema.parse(request.params)
  const task = options.service.getTask(id)
  const open = options.terminals.list(id)
  // Shells that wait to come back after a restart are listed too, so their panes show. They
  // start when a dashboard attaches to them.
  const waiting = (options.openShells?.waiting(id) ?? []).filter((name) => !open.includes(name))
  const terminals = [...open, ...waiting].sort((a, b) => Number(a) - Number(b))
  const restoring = options.openShells?.restoring(id) ?? {}
  // The shells where an agent runs now, so a shell left after a restart is not called the agent's.
  const agents = open.filter((name) => options.stages.agentRuns(id, name))
  hooks.forgetShellsExcept(id, agents)
  // Each agent shows the state of its own stage, not the state of the whole task, and the agent
  // that runs there, which may not be the task's. An agent on no stage of the current step has
  // the state its hooks gave, if any.
  const lanes = Object.fromEntries(
    await Promise.all(
      agents.map(async (name) => {
        const agent = await options.stages.agentIn(id, name)
        const lane = options.stages.laneOfTerminal(task, name)
        const current =
          lane.stage !== null && (lane.stage === task.stage || lane.stage in task.lanes)
        const state = current ? lane.state : (hooks.shellState(id, name) ?? lane.state)
        return [name, { ...lane, state, agent }] as const
      }),
    ),
  )
  // A shell with no agent shows the program in its foreground, such as "zsh".
  const programs = Object.fromEntries(
    open.map((name) => {
      const program = options.terminals.foreground(id, name)
      return [name, program === undefined ? null : basename(program)] as const
    }),
  )
  return { terminals, agents, lanes, programs, restoring }
}

function handleTerminalSocket(
  { options }: RouteDeps,
  socket: TerminalSocket,
  request: FastifyRequest,
): void {
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
  const { cols, rows, terminal, shellId, offset } = query.data
  const attachOptions = { visible: query.data.visible === '1', shellId, offset }
  const client: TerminalClient = {
    send: (frame) => {
      // ws sends a Buffer as a binary frame and a string as a text frame.
      if (frame instanceof Uint8Array) {
        socket.send(frame)
        return
      }
      socket.send(JSON.stringify(frame))
      if (frame.type === 'exit') {
        socket.close()
      }
    },
  }

  let detach: () => void
  try {
    // A client that lost its socket only resumes a shell that runs, never one that waits.
    if (shellId === undefined) {
      options.stages.restoreShell(taskId, terminal)
    }
    detach = options.terminals.attach(taskId, client, { cols, rows }, terminal, attachOptions)
  } catch (error) {
    if (error instanceof ShellGoneError) {
      socket.close(CLOSE_GONE, 'gone')
      return
    }
    const message = error instanceof Error ? error.message : 'The terminal could not start.'
    socket.send(JSON.stringify({ type: 'error', message } satisfies TerminalServerMessage))
    socket.close(CLOSE_REFUSED, 'refused')
    return
  }

  // A client that sent nothing for this long is gone or asleep. A silent peer may never answer
  // a close frame, so the socket is dropped at once.
  let silent: ReturnType<typeof setTimeout> | undefined
  const heard = () => {
    clearTimeout(silent)
    silent = setTimeout(() => socket.terminate(), SILENT_CLOSE_MS)
  }
  heard()

  socket.on('message', (raw: { toString: () => string }, isBinary: boolean) => {
    heard()
    // The dashboard sends only JSON.
    if (isBinary) {
      return
    }
    let data: unknown
    try {
      data = JSON.parse(raw.toString())
    } catch {
      return
    }
    const parsed = terminalClientMessageSchema.safeParse(data)
    if (!parsed.success) {
      return
    }
    const message = parsed.data
    if (message.type === 'input') {
      options.terminals.input(taskId, message.data, terminal, client)
    } else if (message.type === 'resize') {
      const size = { cols: message.cols, rows: message.rows }
      options.terminals.resize(taskId, size, terminal, client)
    } else if (message.type === 'visible') {
      options.terminals.visible(taskId, message, terminal, client)
    } else if (message.type === 'ack') {
      options.terminals.ack(taskId, message.offset, terminal, client)
    } else if (message.type === 'ping') {
      client.send({ type: 'pong' })
    }
  })
  const end = () => {
    clearTimeout(silent)
    detach()
  }
  socket.on('close', end)
  socket.on('error', end)
}
