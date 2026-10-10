import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  DEFAULT_PROJECT_SETTINGS,
  type EventType,
  type ProjectSettings,
  type Task,
} from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import type { RunCommand } from './stage-checks.js'
import { archiveDir, STAGE_DIR } from './stage-files.js'
import {
  issueContext,
  issueNumber,
  LAUNCH_GRACE_MS,
  parseIssueRef,
  StageActionError,
  StageInputError,
  StageRunner,
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
let settings: ProjectSettings
let clock: number
let runCommand: RunCommand
let terminals: TerminalService

beforeEach(() => {
  repo = createTestRepo()
  dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  const bus = new EventBus()
  settings = DEFAULT_PROJECT_SETTINGS
  clock = Date.parse('2026-10-09T09:00:00.000Z')
  runCommand = async () => ({ code: 0, output: '' })
  tasks = new TaskService({ dataDir, index, bus, settingsOf: () => settings })
  workspaces = new WorkspaceService({
    tasks,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
  })
  const spawner = fakeSpawner()
  ptys = spawner.ptys
  terminals = new TerminalService({
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
    runCommand: (command, cwd) => runCommand(command, cwd),
    now: () => clock,
    platform: 'linux',
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
  it('writes the context of a typed task and moves on by itself', async () => {
    const task = await created({ title: 'Export tasks', description: 'As CSV, for reports.' })

    expect(task).toMatchObject({ stage: 'spec', state: 'ready' })
    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context).toContain('# Export tasks')
    expect(context).toContain('As CSV, for reports.')
    expect(tasks.listEvents(task.id).map((entry) => entry.event.type)).toEqual([
      'task.created',
      'stage.started',
      'context.fetched',
      'stage.reported',
      'stage.checked',
      'stage.passed',
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

  it('runs the skill that was picked, of two', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      context: {
        sources: [
          { type: 'manual' },
          { type: 'skill', name: 'Jira', skill: 'jira-ticket' },
          { type: 'skill', name: 'Linear', skill: 'linear-issue' },
        ],
      },
    }
    const waiting = await created({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', name: 'Linear', ref: 'LIN-7' },
    })

    const task = await withWorkspace(waiting.id)
    expect(launchScript(task.id)).toContain('/linear-issue LIN-7')
    expect(launchScript(task.id)).not.toContain('jira-ticket')
  })

  it('writes the context into a workspace made while the issue was read', async () => {
    const task = tasks.createTask({
      title: 'Export',
      context: { source: 'github-issue', ref: '#7' },
    })
    await workspaces.create(task.id)
    await runner.settle()

    const current = tasks.getTask(task.id)
    expect(current.state).toBe('review')
    expect(readFileSync(join(current.workspace ?? '', STAGE_DIR, 'context.md'), 'utf8')).toContain(
      'Dates in UTC, please.',
    )
  })

  it('reads an issue of another repository from its address', async () => {
    const ref = 'https://github.com/other/repo/issues/7#issuecomment-1'
    const task = await created({ title: 'Export', context: { source: 'github-issue', ref } })

    expect(ghCalls[0]).toEqual([
      'issue',
      'view',
      '7',
      '--repo',
      'other/repo',
      '--json',
      'number,title,body,url,labels,comments',
    ])
    expect(task.issue?.key).toBe('other/repo#7')
  })

  it('waits for you when the issue number is missing', async () => {
    const task = await created({ title: 'Export', context: { source: 'github-issue', ref: '' } })
    expect(task.state).toBe('needs-input')
  })
})

describe('agent stages', () => {
  const atSpec = () => created({ title: 'Export tasks', agent: 'claude-code' })

  it('waits for a workspace, then starts the agent with the stage prompt', async () => {
    const waiting = await atSpec()
    expect(waiting).toMatchObject({ stage: 'spec', state: 'ready' })
    expect(ptys).toHaveLength(0)

    const task = await withWorkspace(waiting.id)
    expect(task).toMatchObject({ stage: 'spec', state: 'running' })
    expect(ptys).toHaveLength(1)
    expect(launchScript(task.id)).toContain('spec.md')
    expect(launchScript(task.id)).toContain('natamux done')
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
    expect(runner.takeReport(task.id, '1')).toBe(true)
    expect(runner.takeReport(task.id, '1')).toBe(false)
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
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    return withWorkspace(task.id)
  }

  it('starts each stage in its own terminal: implement in the first, test spec in the next', async () => {
    const task = await atBuild()

    expect(task.lanes).toEqual({ 'test-spec': 'running', implement: 'running' })
    expect(launchScript(task.id)).toContain('Write the tests first')
    expect(launchScript(task.id)).toContain('Do not commit')
    expect(launchScript(task.id, '2')).toContain('test-spec.md')
    expect(runner.laneFor(task, '2')).toEqual({ stage: 'test-spec', state: 'running' })
    expect(runner.laneFor(task, '1')).toEqual({ stage: 'implement', state: 'running' })
  })

  it('finishes implement with changes that are not committed, and leaves them so', async () => {
    const task = await atBuild()
    const workspace = task.workspace ?? ''
    writeFileSync(join(workspace, 'export.ts'), 'export {}\n')

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'running', implement: 'done' })
    expect(tasks.listEvents(task.id).at(-1)?.event.payload.passes).toEqual([])
    expect(runGit(workspace, 'status', '--porcelain')).toContain('export.ts')
  })

  it('asks for the stage name when two stages could be meant', async () => {
    const task = await atBuild()
    await expect(runner.act(task.id, 'fail')).rejects.toThrow(StageActionError)
    const failed = await runner.act(task.id, 'fail', { stage: 'test-spec', reason: 'Stuck' })
    expect(failed.lanes['test-spec']).toBe('failed')
  })
})

describe('helpers', () => {
  it.each([
    ['#42', 42],
    ['42', 42],
    ['https://github.com/o/r/issues/42', 42],
    ['https://github.com/o/r/issues/42#issuecomment-123', 42],
    ['o/r#42', 42],
    ['ABC-42', undefined],
    ['o/r42', undefined],
    ['', undefined],
  ])('reads the issue number in %j', (ref, number) => {
    expect(issueNumber(ref)).toBe(number)
  })

  it('reads the repository of an issue address', () => {
    expect(parseIssueRef('https://github.com/torvalds/linux/issues/1')).toEqual({
      number: 1,
      slug: 'torvalds/linux',
      repo: 'torvalds/linux',
    })
    expect(parseIssueRef('https://ghe.example.com/o/r/issues/5')).toEqual({
      number: 5,
      slug: 'o/r',
      repo: 'ghe.example.com/o/r',
    })
    expect(parseIssueRef('#5')).toEqual({ number: 5 })
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
    // Held at the context gate, so the spec stage starts only when the shell is set up.
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'context' },
    })
    await workspaces.create(task.id)
    // The dashboard opened the first shell; the program in it is up to the test.
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

  it('starts the agent again when it ended and only the shell is left', async () => {
    const { task, pty } = await atSpecWithShell('bash')
    clock += LAUNCH_GRACE_MS + 1

    await runner.act(task.id, 'reopen', { to: 'spec' })

    const typed = pty.written.join('')
    expect(typed.split(`agent-launch-${task.id}.sh`)).toHaveLength(3)
    expect(typed).not.toContain('\x1b[200~')
  })

  it('makes a failed check final when no agent is left to fix it', async () => {
    const { task, pty } = await atSpecWithShell('bash')
    clock += LAUNCH_GRACE_MS + 1

    await runner.act(task.id, 'report')

    expect(tasks.getTask(task.id).state).toBe('needs-input')
    expect(pty.written.join('')).not.toContain('The checks of this stage failed')
  })

  it('starts one agent when a person starts it while the new workspace starts the stage', async () => {
    const task = await created({ title: 'Export tasks' })
    await workspaces.create(task.id)

    const after = await runner.startByHand(task.id, 'claude-code')
    await runner.settle()

    expect(after).toMatchObject({ stage: 'spec', state: 'running' })
    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.written.join('').split(`agent-launch-${task.id}.sh`)).toHaveLength(2)
  })

  it('makes the missing workspace and starts the agent the person picked in it', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    let made = 0
    const makeWorkspace = async () => {
      made += 1
      return workspaces.create(task.id)
    }

    const after = await runner.startByHand(task.id, 'codex', undefined, makeWorkspace)
    await runner.settle()

    expect(after).toMatchObject({ stage: 'spec', state: 'running', agent: 'codex' })
    expect(after.workspace).not.toBeNull()
    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.written.join('').split(`agent-launch-${task.id}.sh`)).toHaveLength(2)

    await runner.startByHand(task.id, 'codex', 'Also add a test', makeWorkspace)
    expect(made).toBe(1)
  })

  it('gives a prompt to the agent that runs, rather than starting another', async () => {
    const task = await withWorkspace((await created({ title: 'Export tasks' })).id)
    const pty = ptys[0] as FakePty
    pty.program = 'node'

    await runner.startByHand(task.id, 'claude-code', 'Also add a test')

    expect(ptys).toHaveLength(1)
    expect(pty.written.join('')).toContain('\x1b[200~Also add a test')
  })

  it('ends the shells of a task that is cancelled', async () => {
    const { task, pty } = await atSpecWithShell('node')

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })

    expect(pty.killed).toBe(true)
    expect(terminals.list(task.id)).toEqual([])
  })
})

describe('a send back', () => {
  async function inReview(): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'review' },
    })
    return withWorkspace(task.id)
  }

  const typesSince = (id: string, type: EventType) => {
    const types = tasks.listEvents(id).map((entry) => entry.event.type)
    return types.slice(types.lastIndexOf(type))
  }

  it('starts the code stage again with the changes asked for', async () => {
    const task = await inReview()

    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'review.changes-requested',
      payload: { reason: 'Fix the dates' },
    })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({
      stage: 'build',
      lanes: { 'test-spec': 'done', implement: 'running' },
    })
    expect(typesSince(task.id, 'review.changes-requested')).toEqual([
      'review.changes-requested',
      'stage.started',
      'agent.launched',
    ])
    expect(launchScript(task.id)).toContain('What to change: Fix the dates')
  })

  it('takes the stage up again, with no new prompt, when you type to the agent', async () => {
    const task = await inReview()
    terminals.run(task.id, 'claude')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'

    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.resumed',
      payload: { stage: 'review' },
    })
    await runner.settle()

    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    expect(typesSince(task.id, 'agent.resumed')).toEqual(['agent.resumed', 'stage.started'])
    expect(pty.written).toEqual(['claude\r'])
  })

  it('refuses a stage the task does not have or has not reached', async () => {
    const task = await inReview()
    await expect(runner.act(task.id, 'reopen', { to: 'nope' })).rejects.toThrow(StageInputError)
    await expect(runner.act(task.id, 'reopen', { to: 'release' })).rejects.toThrow(StageInputError)
  })

  it('answers with the task after its stage failed to start', async () => {
    const task = await created({ title: 'Export', context: { source: 'github-issue', ref: '' } })
    expect(task.state).toBe('needs-input')

    const after = await runner.act(task.id, 'reopen', { to: 'context' })

    expect(after.state).toBe('needs-input')
    expect(typesSince(task.id, 'stage.reopened')).toEqual([
      'stage.reopened',
      'stage.started',
      'stage.check-failed',
    ])
  })
})

describe('a pipeline of its own', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto', check: 'make test' },
        { id: 'review', kind: 'review', gate: 'manual' },
      ],
    }
  })

  async function started(input: Record<string, unknown> = {}): Promise<Task> {
    const task = await created({ title: 'Export', agent: 'claude-code', ...input })
    await withWorkspace(task.id)
    return runner.act(task.id, 'start')
  }

  it('runs the check command at the root of the worktree', async () => {
    const cwds: string[] = []
    runCommand = async (_command, cwd) => {
      cwds.push(cwd)
      return { code: 0, output: '' }
    }
    const task = await started()

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(cwds).toEqual([task.workspace])
    expect(tasks.getTask(task.id).stage).toBe('review')
  })

  it('asks for a commit and checks for it when the settings turn it on', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto', requireCommit: true, requireClean: true },
        { id: 'review', kind: 'review', gate: 'manual' },
      ],
    }
    const task = await started()
    expect(launchScript(task.id)).toContain('Commit your work when it is done.')
    const workspace = task.workspace ?? ''
    writeFileSync(join(workspace, 'export.ts'), 'export {}\n')

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.listEvents(task.id).at(-1)?.event.payload.reasons).toEqual([
      'There is no new commit since the stage started',
      '1 file is not committed',
    ])

    runGit(workspace, 'add', '.')
    runGit(workspace, 'commit', '-m', 'Add the export')
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('review')
  })

  it('drops the result of checks that ran while the stage started again', async () => {
    let finish: (code: number) => void = () => undefined
    runCommand = () =>
      new Promise((resolve) => {
        finish = (code) => resolve({ code, output: 'failed' })
      })
    const task = await started()

    const reporting = runner.act(task.id, 'report', { terminal: '1' })
    await runner.act(task.id, 'reopen', { to: 'implement' })
    finish(1)
    await reporting

    expect(tasks.listEvents(task.id).map((entry) => entry.event.type)).not.toContain(
      'stage.check-failed',
    )
    expect(tasks.getTask(task.id).state).toBe('running')
  })

  it('waits for the tasks it depends on, then starts', async () => {
    const first = await created({ title: 'Schema' })
    const task = await started({ dependsOn: [first.id] })
    expect(task.state).toBe('draft')
    expect(ptys).toHaveLength(0)

    tasks.applyEvent(first.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'done' },
    })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(launchScript(task.id)).toContain('Task: Export')
  })

  it('runs a stage command as one unit, in the same terminal each time', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'testing', kind: 'testing', gate: 'auto', command: 'make test; make lint' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
    const task = await started()
    const script = join(dataDir, `stage-command-${task.id}.sh`)

    expect(terminals.list(task.id)).toEqual(['2'])
    expect(ptys[0]?.written).toEqual([`sh '${script}'\r`])
    expect(readFileSync(script, 'utf8')).toBe(
      [
        'if (',
        'make test; make lint',
        '); then',
        `  natamux stage pass ${task.id} --stage testing`,
        'else',
        `  natamux stage fail ${task.id} --stage testing`,
        'fi',
        '',
      ].join('\n'),
    )

    await runner.act(task.id, 'reopen', { to: 'testing' })

    expect(ptys[0]?.killed).toBe(true)
    expect(ptys).toHaveLength(2)
    expect(terminals.list(task.id)).toEqual(['2'])
  })
})
