import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { claudeCodeHookSettings } from '@helmsman/core'
import { AgentRunner } from './agent-runner.js'
import { loadConfig } from './config.js'
import { EventBus } from './event-bus.js'
import { GenericSignals } from './generic-signals.js'
import { IssueImporter } from './issue-importer.js'
import { spawnShell } from './pty.js'
import { PullRequestWatcher } from './pull-request-watcher.js'
import { RepoService } from './repo-service.js'
import { buildServer } from './server.js'
import { setupCommand } from './setup-script.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { TemplateService } from './template-service.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceService } from './workspace-service.js'

const config = loadConfig(process.env, process.cwd())

mkdirSync(config.dataDir, { recursive: true })
const index = new TaskIndex(join(config.dataDir, 'index.sqlite'))
const bus = new EventBus()
const service = new TaskService({ dataDir: config.dataDir, index, bus })
service.init()
const repos = new RepoService({ dataDir: config.dataDir, defaultPath: config.repoDir })
const workspaces = new WorkspaceService({
  tasks: service,
  repoDir: config.repoDir,
  worktreesDir: config.worktreesDir,
  repos,
  onCreated: (task) => {
    const command = task.workspace === null ? null : setupCommand(task.workspace)
    if (command !== null) {
      terminals.run(task.id, command)
    }
  },
})

// Shells run on the same machine as the daemon, so they reach it on loopback. They get the token
// and the address for `helmsman` and for the hooks of the agents they start.
const daemonUrl = `http://127.0.0.1:${config.port}`
const genericSignals = new GenericSignals(service)
const terminals = new TerminalService({
  spawn: spawnShell,
  workspaceOf: (taskId) => workspaces.workspaceOf(taskId),
  env: { ...process.env, HELMSMAN_TOKEN: config.token, HELMSMAN_URL: daemonUrl },
  onOutput: (taskId, terminal, data) => genericSignals.onOutput(taskId, terminal, data),
})

const claudeSettingsPath = join(config.dataDir, 'claude-code-settings.json')
writeFileSync(claudeSettingsPath, `${JSON.stringify(claudeCodeHookSettings(daemonUrl), null, 2)}\n`)
const agents = new AgentRunner({
  tasks: service,
  terminals,
  claudeSettingsPath,
  claudeCommand: process.env.HELMSMAN_CLAUDE_COMMAND,
})

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
  token: config.token,
  webIndexPath: config.webIndexPath,
})
await app.listen({ host: config.host, port: config.port })

console.log(`Helmsman daemon listening on http://${config.host}:${config.port}`)
console.log(`Data directory: ${config.dataDir}`)
console.log(`Repository: ${config.repoDir}`)
if (config.tokenWasGenerated) {
  console.log(`Token for this run: ${config.token}`)
}

async function shutdown(): Promise<void> {
  terminals.closeAll()
  pullRequests.stop()
  await app.close()
  index.close()
  process.exit(0)
}

process.on('SIGINT', shutdown)
process.on('SIGTERM', shutdown)
