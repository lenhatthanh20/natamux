import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { claudeCodeHookSettings } from '@natamux/core'
import { geminiHookSettings, LAUNCHABLE_AGENTS } from './agent-launch.js'
import { AgentRunner } from './agent-runner.js'
import { loadConfig } from './config.js'
import { EventBus } from './event-bus.js'
import { GenericSignals } from './generic-signals.js'
import { IssueImporter } from './issue-importer.js'
import { IssueLinker } from './issue-linker.js'
import { ProjectSettingsService } from './project-settings.js'
import { spawnShell } from './pty.js'
import { PullRequestWatcher } from './pull-request-watcher.js'
import { RepoService } from './repo-service.js'
import { buildServer } from './server.js'
import { setupCommand } from './setup-script.js'
import { StageRunner } from './stage-runner.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { TemplateService } from './template-service.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceService } from './workspace-service.js'

const config = loadConfig(process.env, process.cwd())

mkdirSync(config.dataDir, { recursive: true })
const index = new TaskIndex(join(config.dataDir, 'index.sqlite'))
const bus = new EventBus()
const repos = new RepoService({
  dataDir: config.dataDir,
  defaultPath: config.repoDir,
  tasks: () => service.listTasks(),
})
const settings = new ProjectSettingsService({ repos })
const service = new TaskService({
  dataDir: config.dataDir,
  index,
  bus,
  settingsOf: (repo) => settings.get(repo),
})
service.init()
const issueLinker = new IssueLinker({
  tasks: service,
  bus,
  dataDir: config.dataDir,
  repoPath: (repo) => repos.pathOf(repo),
})
issueLinker.start()
const workspaces = new WorkspaceService({
  tasks: service,
  repoDir: config.repoDir,
  worktreesDir: config.worktreesDir,
  repos,
  pullRequestBody: (task) => issueLinker.pullRequestBody(task),
  closeTerminals: (taskId) => terminals.close(taskId),
  onCreated: (task) => {
    const command = task.workspace === null ? null : setupCommand(task.workspace)
    if (command !== null) {
      terminals.run(task.id, command)
    }
  },
})

// Shells run on the same machine as the daemon, so they reach it on loopback. They get the token
// and the address for `natamux` and for the hooks of the agents they start.
const daemonUrl = `http://127.0.0.1:${config.port}`
const genericSignals = new GenericSignals(service)
const terminals = new TerminalService({
  spawn: spawnShell,
  workspaceOf: (taskId) => workspaces.workspaceOf(taskId),
  env: { ...process.env, NATAMUX_TOKEN: config.token, NATAMUX_URL: daemonUrl },
  onOutput: (taskId, terminal, data) => genericSignals.onOutput(taskId, terminal, data),
})

const claudeSettingsPath = join(config.dataDir, 'claude-code-settings.json')
writeFileSync(claudeSettingsPath, `${JSON.stringify(claudeCodeHookSettings(daemonUrl), null, 2)}\n`)
const geminiSettingsPath = join(config.dataDir, 'gemini-settings.json')
writeFileSync(geminiSettingsPath, `${JSON.stringify(geminiHookSettings(), null, 2)}\n`)
// NATAMUX_CLAUDE_COMMAND, NATAMUX_CODEX_ARGS and so on: the agent id in capitals.
const perAgent = (suffix: string) =>
  Object.fromEntries(
    LAUNCHABLE_AGENTS.map((agent) => [
      agent,
      process.env[`NATAMUX_${agent === 'claude-code' ? 'CLAUDE' : agent.toUpperCase()}_${suffix}`],
    ]),
  )
const agents = new AgentRunner({
  tasks: service,
  terminals,
  claudeSettingsPath,
  geminiSettingsPath,
  programs: perAgent('COMMAND'),
  extraArgs: perAgent('ARGS'),
  onLaunch: (launch) => genericSignals.watchLaunch(launch),
})

const stages = new StageRunner({
  tasks: service,
  terminals,
  agents,
  bus,
  dataDir: config.dataDir,
  repoPath: (repo) => repos.pathOf(repo),
})
stages.start()

const pullRequests = new PullRequestWatcher({ tasks: service, cwd: config.repoDir })
pullRequests.start()

const app = await buildServer({
  service,
  workspaces,
  terminals,
  agents,
  pullRequests,
  issues: new IssueImporter({ tasks: service, repoPath: (repo) => repos.pathOf(repo) }),
  templates: new TemplateService({ dataDir: config.dataDir, tasks: service }),
  repos,
  bus,
  stages,
  settings,
  issueLinker,
  dataDir: config.dataDir,
  token: config.token,
  webIndexPath: config.webIndexPath,
  webDir: config.webDir,
})
await app.listen({ host: config.host, port: config.port })

console.log(`Natamux daemon listening on http://${config.host}:${config.port}`)
console.log(`Data directory: ${config.dataDir}`)
console.log(`Repository: ${config.repoDir}`)
if (config.tokenWasGenerated) {
  console.log(`Token for this run: ${config.token}`)
}

async function shutdown(): Promise<void> {
  terminals.closeAll()
  stages.stop()
  issueLinker.stop()
  pullRequests.stop()
  await app.close()
  index.close()
  process.exit(0)
}

process.on('SIGINT', shutdown)
process.on('SIGTERM', shutdown)
