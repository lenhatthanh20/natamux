import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { DEFAULT_PIPELINE, DEFAULT_PROJECT_SETTINGS, type Task } from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import { archiveDir, STAGE_DIR } from './stage-files.js'
import {
  issueContext,
  issueNumber,
  StageActionError,
  StageRunner,
  stageOfTerminal,
  terminalOfStage,
} from './stage-runner.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceService } from './workspace-service.js'

const ISSUE = {
  number: 7,
  title: 'Export tasks as CSV',
  body: 'We need a CSV export.',
  url: 'https://github.com/o/r/issues/7',
  labels: [{ name: 'feature' }],
  comments: [{ author: { login: 'sam' }, body: 'Dates in UTC, please.' }],
}

let repo: TestRepo
let dataDir: string
let index: TaskIndex
let tasks: TaskService
let workspaces: WorkspaceService
let runner: StageRunner
let ptys: FakePty[]
let ghCalls: string[][]

beforeEach(() => {
  repo = createTestRepo()
  dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  const bus = new EventBus()
  tasks = new TaskService({ dataDir, index, bus, settingsOf: () => DEFAULT_PROJECT_SETTINGS })
  workspaces = new WorkspaceService({
    tasks,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
  })
  const spawner = fakeSpawner()
  ptys = spawner.ptys
  const terminals = new TerminalService({
    spawn: spawner.spawn,
    workspaceOf: (taskId) => workspaces.workspaceOf(taskId),
  })
  const agents = new AgentRunner({
    tasks,
    terminals,
    claudeSettingsPath: join(dataDir, 'settings.json'),
    platform: 'linux',
  })
  ghCalls = []
  runner = new StageRunner({
    tasks,
    terminals,
    agents,
    bus,
    dataDir,
    repoPath: () => repo.dir,
    gh: async (_cwd, args) => {
      ghCalls.push([...args])
      return JSON.stringify(ISSUE)
    },
  })
  runner.start()
})

afterEach(() => {
  runner.stop()
  index.close()
  repo.cleanup()
})

async function created(input: Record<string, unknown>): Promise<Task> {
  const task = tasks.createTask(input)
  await runner.settle()
  return tasks.getTask(task.id)
}

async function withWorkspace(id: string): Promise<Task> {
  await workspaces.create(id)
  await runner.settle()
  return tasks.getTask(id)
}

const launchScript = (id: string, terminal = '1') =>
  readFileSync(
    join(dataDir, `agent-launch-${id}${terminal === '1' ? '' : `-${terminal}`}.sh`),
    'utf8',
  )

function writeWorking(task: Task, name: string, text: string): void {
  const dir = join(task.workspace ?? '', STAGE_DIR)
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, name), text)
}

describe('context', () => {
  it('writes the context of a typed task and waits for your approval', async () => {
    const task = await created({ title: 'Export tasks', description: 'As CSV, for reports.' })

    expect(task).toMatchObject({ stage: 'context', state: 'review' })
    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context).toContain('# Export tasks')
    expect(context).toContain('As CSV, for reports.')
    expect(tasks.listEvents(task.id).map((entry) => entry.event.type)).toEqual([
      'task.created',
      'stage.started',
      'context.fetched',
      'stage.reported',
      'stage.checked',
    ])
  })

  it('reads a GitHub issue with its comments and links the issue', async () => {
    const task = await created({ title: 'Export', context: { source: 'github-issue', ref: '#7' } })

    expect(task.state).toBe('review')
    expect(task.issue).toEqual({ provider: 'github', key: '#7', url: ISSUE.url })
    expect(ghCalls[0]?.slice(0, 3)).toEqual(['issue', 'view', '7'])
    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context).toContain('Dates in UTC, please.')
  })

  it('writes the context into a workspace made while the issue was read', async () => {
    const task = tasks.createTask({ title: 'Export', context: { source: 'github-issue', ref: '#7' } })
    await workspaces.create(task.id)
    await runner.settle()

    const current = tasks.getTask(task.id)
    expect(current.state).toBe('review')
    expect(readFileSync(join(current.workspace ?? '', STAGE_DIR, 'context.md'), 'utf8')).toContain(
      'Dates in UTC, please.',
    )
  })

  it('waits for you when the issue number is missing', async () => {
    const task = await created({ title: 'Export', context: { source: 'github-issue', ref: '' } })
    expect(task.state).toBe('needs-input')
  })
})

describe('agent stages', () => {
  async function atSpec(): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await runner.act(task.id, 'approve')
    await runner.settle()
    return tasks.getTask(task.id)
  }

  it('waits for a workspace, then starts the agent with the stage prompt', async () => {
    const waiting = await atSpec()
    expect(waiting).toMatchObject({ stage: 'spec', state: 'ready' })
    expect(ptys).toHaveLength(0)

    const task = await withWorkspace(waiting.id)
    expect(task).toMatchObject({ stage: 'spec', state: 'running' })
    expect(ptys).toHaveLength(1)
    expect(launchScript(task.id)).toContain('spec.md')
    expect(launchScript(task.id)).toContain('helmsman done')
    expect(readFileSync(join(task.workspace ?? '', STAGE_DIR, 'context.md'), 'utf8')).toContain(
      'Export tasks',
    )
  })

  it('sends failed checks back to the agent once, then waits for you', async () => {
    const task = await withWorkspace((await atSpec()).id)

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).state).toBe('running')
    expect(ptys[0]?.written.join('')).toContain('spec.md is missing')

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).state).toBe('needs-input')
  })

  it('waits for approval when the output has its sections, and drops the turn end after', async () => {
    const task = await withWorkspace((await atSpec()).id)
    writeWorking(
      task,
      'spec.md',
      '# Goal\nx\n# Scope\nx\n# Out of scope\nx\n# Acceptance criteria\nx\n',
    )

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'review' })
    expect(runner.takeReport(task.id, 'spec')).toBe(true)
    expect(runner.takeReport(task.id, 'spec')).toBe(false)
  })

  it('names the missing sections', async () => {
    const task = await withWorkspace((await atSpec()).id)
    writeWorking(task, 'spec.md', '# Goal\nx\n')

    await runner.act(task.id, 'report', { terminal: '1' })
    const failed = tasks.listEvents(task.id).at(-1)?.event
    expect(failed?.type).toBe('stage.check-failed')
    expect(failed?.payload.reasons).toEqual([
      'spec.md has no Scope, Out of scope, Acceptance criteria section',
    ])
  })
})

describe('a group', () => {
  async function atBuild(): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await withWorkspace(task.id)
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    await runner.settle()
    return tasks.getTask(task.id)
  }

  it('starts each stage in its own terminal: implement in the first, test spec in the next', async () => {
    const task = await atBuild()

    expect(task.lanes).toEqual({ 'test-spec': 'running', implement: 'running' })
    expect(launchScript(task.id)).toContain('Write the tests first')
    expect(launchScript(task.id, '2')).toContain('test-spec.md')
    expect(runner.laneFor(task, '2')).toEqual({ stage: 'test-spec', state: 'running' })
    expect(runner.laneFor(task, '1')).toEqual({ stage: 'implement', state: 'running' })
  })

  it('needs a new commit and a clean worktree to finish implement', async () => {
    const task = await atBuild()
    const workspace = task.workspace ?? ''
    writeFileSync(join(workspace, 'export.ts'), 'export {}\n')
    runGit(workspace, 'add', '.')
    runGit(workspace, 'commit', '-m', 'Add the export')

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'running', implement: 'done' })
    expect(tasks.listEvents(task.id).at(-1)?.event.payload.passes).toEqual([
      '1 new commit since the stage started',
      'Every change is committed',
    ])
  })

  it('asks for the stage name when two stages could be meant', async () => {
    const task = await atBuild()
    await expect(runner.act(task.id, 'fail')).rejects.toThrow(StageActionError)
    const failed = await runner.act(task.id, 'fail', { stage: 'test-spec', reason: 'Stuck' })
    expect(failed.lanes['test-spec']).toBe('failed')
  })
})

describe('helpers', () => {
  const build = DEFAULT_PIPELINE[3]

  it('gives the code stage of a group the first terminal', () => {
    if (build === undefined) {
      throw new Error('The default pipeline changed')
    }
    expect(terminalOfStage(build, 'implement')).toBe('1')
    expect(terminalOfStage(build, 'test-spec')).toBe('2')
    expect(stageOfTerminal(build, '2')?.id).toBe('test-spec')
    expect(stageOfTerminal(build, '3')).toBeUndefined()
  })

  it.each([
    ['#42', 42],
    ['42', 42],
    ['https://github.com/o/r/issues/42', 42],
    ['ABC-42', undefined],
    ['', undefined],
  ])('reads the issue number in %j', (ref, number) => {
    expect(issueNumber(ref)).toBe(number)
  })

  it('writes an issue as markdown', () => {
    const text = issueContext(ISSUE)
    expect(text).toContain('# Export tasks as CSV')
    expect(text).toContain('Labels: feature')
    expect(text).toContain('### sam')
  })
})

describe('a prompt for a running agent', () => {
  async function atSpecWithShell(program: string): Promise<{ task: Task; pty: FakePty }> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await workspaces.create(task.id)
    // The dashboard opened the first shell; the program in it is up to the test.
    const terminals = (runner as unknown as { terminals: TerminalService }).terminals
    terminals.run(task.id, 'echo ready')
    const pty = ptys.at(-1) as FakePty
    pty.program = program
    await runner.act(task.id, 'approve')
    await runner.settle()
    return { task: tasks.getTask(task.id), pty }
  }

  it('starts an agent when only a shell runs, and never pastes the prompt into it', async () => {
    const { task, pty } = await atSpecWithShell('bash')
    expect(task).toMatchObject({ stage: 'spec', state: 'running' })
    const typed = pty.written.join('')
    expect(typed).toContain(`agent-launch-${task.id}.sh`)
    expect(typed).not.toContain('\x1b[200~')
  })

  it('pastes the prompt when an agent runs in the foreground', async () => {
    const { pty } = await atSpecWithShell('node')
    expect(pty.written.join('')).toContain('\x1b[200~Task: Export tasks')
  })
})
