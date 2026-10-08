import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { loadConfig } from './config.js'
import { EventBus } from './event-bus.js'
import { spawnShell } from './pty.js'
import { buildServer } from './server.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceService } from './workspace-service.js'

const config = loadConfig(process.env, process.cwd())

mkdirSync(config.dataDir, { recursive: true })
const index = new TaskIndex(join(config.dataDir, 'index.sqlite'))
const bus = new EventBus()
const service = new TaskService({ dataDir: config.dataDir, index, bus })
service.init()
const workspaces = new WorkspaceService({
  tasks: service,
  repoDir: config.repoDir,
  worktreesDir: config.worktreesDir,
})

const terminals = new TerminalService({
  spawn: spawnShell,
  workspaceOf: (taskId) => workspaces.workspaceOf(taskId),
})

const app = await buildServer({ service, workspaces, terminals, bus, token: config.token })
await app.listen({ host: config.host, port: config.port })

console.log(`Helmsman daemon listening on http://${config.host}:${config.port}`)
console.log(`Data directory: ${config.dataDir}`)
console.log(`Repository: ${config.repoDir}`)
if (config.tokenWasGenerated) {
  console.log(`Token for this run: ${config.token}`)
}

async function shutdown(): Promise<void> {
  terminals.closeAll()
  await app.close()
  index.close()
  process.exit(0)
}

process.on('SIGINT', shutdown)
process.on('SIGTERM', shutdown)
