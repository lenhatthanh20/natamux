import { TASK_ID_HEADER, TERMINAL_HEADER } from '@natamux/core'
import type { FastifyInstance } from 'fastify'
import { z } from 'zod'
import { UnknownAgentError } from '../../agents/agent-hook-service.js'
import { LAUNCHABLE_AGENTS } from '../../agents/agent-runner.js'
import { COPILOT_EVENT_FIELD } from '../../agents/copilot-hooks.js'
import { StageActionError } from '../../stages/stage-actions.js'
import { MAIN_TERMINAL } from '../../terminals/terminal-limits.js'
import type { RouteDeps } from '../deps.js'
import { taskParamsSchema } from './tasks.js'

const hookParamsSchema = z.object({ agent: z.string().min(1) })
const hookQuerySchema = z.object({ event: z.string().min(1).max(64).optional() })
const messageBodySchema = z.object({ text: z.string().trim().min(1).max(20_000) })
const startAgentBodySchema = z.object({
  agent: z.string().min(1).max(64),
  prompt: z.string().max(20_000).optional(),
  base: z.string().min(1).max(255).optional(),
})

export function registerAgentRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options, hooks } = deps

  // What the agent of each task does now, from its last hook.
  app.get('/activity', async () => ({ activity: hooks.activity() }))

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

  app.post('/tasks/:id/agent', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { agent, prompt, base } = startAgentBodySchema.parse(request.body)
    // Checked before a workspace is made for nothing.
    if (!(LAUNCHABLE_AGENTS as readonly string[]).includes(agent)) {
      throw new UnknownAgentError(agent)
    }
    const makeWorkspace = () => options.workspaces.create(id, base)
    return { task: await options.stages.startByHand(id, agent, prompt, makeWorkspace) }
  })

  // The agent of Terminal 1 of a done or cancelled task, on its last session.
  app.post('/tasks/:id/agent/reopen', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    return { task: options.stages.reopenAgent(id) }
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
}
