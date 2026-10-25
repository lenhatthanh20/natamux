import { taskIdSchema } from '@natamux/core'
import type { FastifyInstance } from 'fastify'
import { z } from 'zod'
import { STAGE_ACTIONS, type StageAction } from '../../stages/stage-actions.js'
import { listStageFiles, readStageFile, writeStageFile } from '../../stages/stage-files.js'
import { AgentRefusedError } from '../auth.js'
import type { RouteDeps } from '../deps.js'
import { taskParamsSchema } from './tasks.js'

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
// What only a person may do, so an agent cannot pass a manual gate by itself.
const AGENT_REFUSED_ACTIONS: readonly StageAction[] = ['approve', 'skip', 'pass']
const workflowBodySchema = z.object({ stage: z.string().min(1).max(40), on: z.boolean() })
const stageFileParamsSchema = z.object({ id: taskIdSchema, name: z.string().min(1).max(100) })
const stageFileBodySchema = z.object({ text: z.string() })

export function registerStageRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options, byAgent, refuseAgent } = deps

  // Moves a task through its stages: approve, send back, skip, pass, fail, report done, start.
  app.post('/tasks/:id/stage', async (request) => {
    const { id } = taskParamsSchema.parse(request.params)
    const { action, ...input } = stageBodySchema.parse(request.body)
    const agent = byAgent(request)
    if (agent && AGENT_REFUSED_ACTIONS.includes(action)) {
      throw new AgentRefusedError(`${action} a stage`)
    }
    return { task: await options.stages.act(id, action, { ...input, byAgent: agent }) }
  })

  // Whether a stage runs as a workflow the next time it starts.
  app.put('/tasks/:id/workflow', async (request) => {
    refuseAgent(request, 'turn a workflow on or off')
    const { id } = taskParamsSchema.parse(request.params)
    const { stage, on } = workflowBodySchema.parse(request.body)
    return { task: options.service.setWorkflow(id, stage, on) }
  })

  // Whether a stage waits for CI before it passes.
  app.put('/tasks/:id/ci', async (request) => {
    refuseAgent(request, 'turn a CI check on or off')
    const { id } = taskParamsSchema.parse(request.params)
    const { stage, on } = workflowBodySchema.parse(request.body)
    return { task: options.service.setCi(id, stage, on) }
  })

  // Whether a later stage is skipped when the task reaches it.
  app.put('/tasks/:id/skip', async (request) => {
    refuseAgent(request, 'mark a stage to skip')
    const { id } = taskParamsSchema.parse(request.params)
    const { stage, on } = workflowBodySchema.parse(request.body)
    return { task: options.service.setSkip(id, stage, on) }
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
}
