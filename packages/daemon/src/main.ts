import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { claudeCodeHookSettings } from '@natamux/core'
import { geminiHookSettings, LAUNCHABLE_AGENTS, prepareAgentHooks } from './agents/agent-launch.js'
import { AgentRunner } from './agents/agent-runner.js'
import { GenericSignals } from './agents/generic-signals.js'
import { UsageService } from './agents/usage-service.js'
import { loadConfig, shellEnv } from './config.js'
import { IssueImporter } from './github/issue-importer.js'
import { IssueLinker } from './github/issue-linker.js'
import { PullRequestWatcher } from './github/pull-request-watcher.js'
import { buildServer } from './http/server.js'
import { ProjectSettingsService } from './projects/project-settings.js'
import { RepoService, repoFolderOr } from './projects/repo-service.js'
import { EventBus } from './shared/event-bus.js'
import { Jobs } from './shared/jobs.js'
import { StageRunner } from './stages/stage-runner.js'
import { type Mark, startupLine } from './startup-marks.js'
import { isTaskEnded } from './tasks/task-helpers.js'
import { TaskIndex } from './tasks/task-index.js'
import { TaskService } from './tasks/task-service.js'
import { TemplateService } from './tasks/template-service.js'
import { type ParentPort, waitForLoginPath } from './terminals/login-path.js'
import { OpenShells } from './terminals/open-shells.js'
import { spawnShell } from './terminals/pty.js'
import { TerminalService } from './terminals/terminal-service.js'
import { setupCommand } from './workspaces/setup-script.js'
import { WorkspaceService } from './workspaces/workspace-service.js'

// Marks of the start, logged as one line after the listen. performance.now() counts from the start
// of the process, so the first mark also shows the time that the imports took.
const marks: Mark[] = []
function mark(name: string): void {
  performance.mark(`daemon:${name}`)
  marks.push([name, Math.round(performance.now())])
}
mark('module-start')

// Set under the desktop app only.
const parentPort = (process as NodeJS.Process & { parentPort?: ParentPort }).parentPort
let loginPath: string | undefined
const pathReady = waitForLoginPath(process.env, parentPort).then((path) => {
  loginPath = path
})
const config = loadConfig(process.env, process.cwd())

mkdirSync(config.dataDir, { recursive: true })
const index = new TaskIndex(join(config.dataDir, 'index.sqlite'))
const bus = new EventBus()
const jobs = new Jobs(bus)
const repos = new RepoService({
  dataDir: config.dataDir,
  ...(config.repoDir === undefined ? {} : { defaultPath: config.repoDir }),
  worktreesDir: config.worktreesDir,
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
mark('init')
const issueLinker = new IssueLinker({
  tasks: service,
  bus,
  dataDir: config.dataDir,
  repoPath: (repo) => repos.pathOf(repo),
  jobs,
})
issueLinker.start()
const workspaces = new WorkspaceService({
  dataDir: config.dataDir,
  tasks: service,
  worktreesDir: config.worktreesDir,
  repos,
  pullRequestBody: (task) => issueLinker.pullRequestBody(task),
  closeTerminals: (taskId) => terminals.closeAndWait(taskId),
  programOf: (agent) => agents.programOf(agent),
  bus,
  jobs,
  onCreated: (task) => {
    const command = task.workspace === null ? null : setupCommand(task.workspace)
    if (command !== null) {
      terminals.run(task.id, command)
    }
  },
})

// Shells run on the same machine as the daemon, so they reach it on loopback. They get the agent
// token and the address for `natamux` and for the hooks of the agents they start.
const daemonUrl = `http://127.0.0.1:${config.port}`
const claudeSettings = `${JSON.stringify(claudeCodeHookSettings(daemonUrl), null, 2)}\n`
const genericSignals = new GenericSignals(service)
const terminals = new TerminalService({
  // An agent the user starts by hand in the shell finds its hooks there.
  spawn: (options) => {
    prepareAgentHooks(options.cwd, { claudeCode: true })
    // The service copied the environment before the login PATH came.
    const env = loginPath === undefined ? options.env : { ...options.env, PATH: loginPath }
    return spawnShell({ ...options, env })
  },
  workspaceOf: (taskId) => workspaces.workspaceOf(taskId),
  env: shellEnv(process.env, config, daemonUrl),
  onOutput: (taskId, terminal, data) => {
    stages.onOutput(taskId, terminal, data)
    genericSignals.onOutput(taskId, terminal, data)
  },
  isEnded: (taskId) => isTaskEnded(service, taskId),
  bus,
  onOutputIdle: (taskId) => workspaces.touch(taskId),
})

const claudeSettingsPath = join(config.dataDir, 'claude-code-settings.json')
writeFileSync(claudeSettingsPath, claudeSettings)
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
  workspaceHooks: { claudeCode: true },
  programs: perAgent('COMMAND'),
  extraArgs: perAgent('ARGS'),
  binDir: process.env.NATAMUX_BIN_DIR,
  onLaunch: (launch) => {
    genericSignals.watchLaunch(launch)
    openShells.noteLaunch(launch.taskId, launch.terminal, launch.agent, launch.session)
  },
})
const openShells = new OpenShells({
  dataDir: config.dataDir,
  terminals,
  bus,
  isEnded: (taskId) => isTaskEnded(service, taskId),
  agentRuns: (taskId, terminal) => stages.agentRuns(taskId, terminal),
  agentIn: (taskId, terminal) => stages.agentIn(taskId, terminal),
})

const stages = new StageRunner({
  tasks: service,
  terminals,
  agents,
  bus,
  dataDir: config.dataDir,
  repoPath: (repo) => repos.pathOf(repo),
  openShells,
  jobs,
})
// Before the listen, so no dashboard attaches to a shell before it is known to wait.
openShells.start()
stages.loadOpenShells()
const pullRequests = new PullRequestWatcher({
  tasks: service,
  cwdOf: repoFolderOr(repos, config.dataDir),
  bus,
})
// They run git, gh and the checks of stages.
const ready = pathReady.then(() => {
  stages.start()
  pullRequests.start()
})

const app = await buildServer({
  service,
  workspaces,
  terminals,
  agents,
  pullRequests,
  issues: new IssueImporter({ tasks: service, repoPath: (repo) => repos.pathOf(repo), jobs }),
  templates: new TemplateService({ dataDir: config.dataDir, tasks: service }),
  repos,
  bus,
  stages,
  openShells,
  settings,
  issueLinker,
  jobs,
  usage: new UsageService({ dataDir: config.dataDir, bus }),
  dataDir: config.dataDir,
  token: config.token,
  agentToken: config.agentToken,
  webIndexPath: config.webIndexPath,
  webDir: config.webDir,
  ready,
})
mark('build-server')
await app.listen({ host: config.host, port: config.port })
mark('listen')
// The desktop app opens its window now, instead of at its next look at /health.
parentPort?.postMessage('ready')
// After the listen, so the hooks of the agents reach the daemon.
void ready.then(() => stages.resumeAgents())

console.log(startupLine(marks))
console.log(`Natamux daemon listening on http://${config.host}:${config.port}`)
console.log(`Data directory: ${config.dataDir}`)
console.log(`Projects: ${repos.list().length}`)
if (config.tokenWasGenerated) {
  console.log(`Token for this run: ${config.token}`)
}

let shuttingDown = false

async function shutdown(): Promise<void> {
  // A second signal must not exit before the shells have ended.
  if (shuttingDown) {
    return
  }
  shuttingDown = true
  // Before the shells end, so they all come back at the next start.
  openShells.stop()
  const shellsEnded = terminals.closeAllAndWait()
  stages.stop()
  issueLinker.stop()
  pullRequests.stop()
  await Promise.all([shellsEnded, app.close()])
  index.close()
  process.exit(0)
}

process.on('SIGINT', shutdown)
process.on('SIGTERM', shutdown)
