import type { FastifyRequest } from 'fastify'
import type { AgentHookService } from '../agents/agent-hook-service.js'
import type { AgentRunner } from '../agents/agent-runner.js'
import type { IssueImporter } from '../github/issue-importer.js'
import type { IssueLinker } from '../github/issue-linker.js'
import type { PullRequestWatcher } from '../github/pull-request-watcher.js'
import type { ProjectSettingsService } from '../projects/project-settings.js'
import type { RepoService } from '../projects/repo-service.js'
import type { EventBus } from '../shared/event-bus.js'
import type { Jobs } from '../shared/jobs.js'
import type { StageRunner } from '../stages/stage-runner.js'
import type { TaskService } from '../tasks/task-service.js'
import type { TemplateService } from '../tasks/template-service.js'
import type { OpenShells } from '../terminals/open-shells.js'
import type { TerminalService } from '../terminals/terminal-service.js'
import type { WorkspaceService } from '../workspaces/workspace-service.js'

export type ServerOptions = {
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
  // The shells that wait to come back after a restart. None in most tests.
  openShells?: OpenShells
  settings: ProjectSettingsService
  issueLinker: IssueLinker
  // The background jobs that run now. None in most tests.
  jobs?: Jobs
  // The daemon's data folder, where stage files are kept for tasks without a worktree.
  dataDir: string
  token: string
  // The token of the task shells. What it sends counts as the agent's, and it may not pass a gate.
  agentToken: string
  // The built dashboard's index.html. With it, an address of the dashboard that is not an API
  // route (a deep link) answers with that page when the browser asks for HTML.
  webIndexPath?: string
  // The built dashboard. Set: the daemon serves its files too, at the same address as the API.
  webDir?: string
  // Where agents keep the user's skills. The home folder by default.
  skillsHome?: string
  // Routes that may run the user's programs wait for it: under the desktop app the PATH of the
  // login shell comes after the start (see login-path.ts). Ready at once by default.
  ready?: Promise<unknown>
}

export type RouteDeps = {
  options: ServerOptions
  hooks: AgentHookService
  byAgent: (request: FastifyRequest) => boolean
  refuseAgent: (request: FastifyRequest, what: string) => void
}
