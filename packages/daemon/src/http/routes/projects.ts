import { homedir } from 'node:os'
import { repoIdSchema } from '@natamux/core'
import type { FastifyInstance } from 'fastify'
import { z } from 'zod'
import { listSkills } from '../../agents/agent-skills.js'
import { singleHeader } from '../auth.js'
import type { RouteDeps } from '../deps.js'

const importIssuesBodySchema = z.object({
  numbers: z.array(z.number().int().positive()).min(1).max(100),
  repo: repoIdSchema.optional(),
})
const repoQuerySchema = z.object({ repo: repoIdSchema.optional() })
// A task may name its repository as null, like none: the default one.
export const taskRepoBodySchema = z.object({ repo: repoIdSchema.nullish() })
const repoParamsSchema = z.object({ id: repoIdSchema })

export function registerProjectRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options, refuseAgent } = deps

  app.get('/repos', async () => ({
    repos: options.repos.list(),
    worktreesDir: options.repos.worktreesDir,
  }))

  app.post('/repos', async (request, reply) => {
    const repo = await options.repos.add(request.body)
    options.bus.publish({ kind: 'repos.changed' })
    return reply.code(201).send({ repo })
  })

  app.delete('/repos/:id', async (request, reply) => {
    const { id } = repoParamsSchema.parse(request.params)
    options.repos.remove(id)
    options.bus.publish({ kind: 'repos.changed' })
    return reply.code(204).send()
  })

  app.get('/git/branches', async (request) => {
    const { repo } = repoQuerySchema.parse(request.query)
    return options.workspaces.branches(repo ?? null)
  })

  app.get('/github/issues', async (request) => {
    const { repo } = repoQuerySchema.parse(request.query)
    return { issues: await options.issues.list(repo ?? null) }
  })

  app.post('/github/issues/import', async (request) => {
    const { numbers, repo } = importIssuesBodySchema.parse(request.body)
    return { tasks: await options.issues.import(numbers, repo ?? null) }
  })

  app.get('/repos/:id/settings', async (request) => {
    const { id } = repoParamsSchema.parse(request.params)
    const loaded = options.settings.load(id)
    return { ...loaded, path: options.settings.pathOf(id) }
  })

  // `If-Match` holds the version the editor read; a file that changed since is not replaced.
  app.put('/repos/:id/settings', async (request) => {
    refuseAgent(request, 'change project settings')
    const { id } = repoParamsSchema.parse(request.params)
    const version = request.headers['if-match']
    const settings = options.settings.save(id, request.body, singleHeader(version))
    return { settings, version: options.settings.load(id).version }
  })

  // The skills of every agent for this repository, for the skill picker of the settings page.
  app.get('/repos/:id/skills', async (request) => {
    const { id } = repoParamsSchema.parse(request.params)
    return listSkills(options.repos.pathOf(id), options.skillsHome ?? homedir())
  })
}
