import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  DEFAULT_PROJECT_SETTINGS,
  type EventType,
  type ProjectSettings,
  type Stage,
  type Task,
} from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { decodeAttachments } from './attachments.js'
import type { CiResult } from './ci-check.js'
import { EventBus } from './event-bus.js'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import type { RunCommand } from './stage-checks.js'
import { archiveDir, STAGE_DIR } from './stage-files.js'
import { WORKFLOW_LINE } from './stage-prompts.js'
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
let agents: AgentRunner
let ptys: FakePty[]
let ghCalls: string[][]
let settings: ProjectSettings
let clock: number
let runCommand: RunCommand
let terminals: TerminalService
let bus: EventBus
// The CI checks asked for, and the way to end the last one.
let ciCalls: { worktree: string; branch: string }[]
let finishCi: (result: CiResult) => void

beforeEach(() => {
  repo = createTestRepo()
  dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  bus = new EventBus()
  settings = DEFAULT_PROJECT_SETTINGS
  clock = Date.parse('2026-10-09T09:00:00.000Z')
  runCommand = async () => ({ code: 0, output: '' })
  ciCalls = []
  finishCi = () => undefined
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
  agents = new AgentRunner({
    tasks,
    terminals,
    claudeSettingsPath: join(dataDir, 'settings.json'),
    platform: 'linux',
  })
  ghCalls = []
  runner = makeRunner(agents)
  runner.start()
})

function makeRunner(agents: AgentRunner): StageRunner {
  return new StageRunner({
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
    // The skills of the machine that runs the tests must not count.
    home: join(repo.root, 'home'),
    checkCi: (input) => {
      ciCalls.push(input)
      return new Promise((resolve) => {
        finishCi = resolve
      })
    },
  })
}

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

// The agent got the text as a paste, and its own Enter as the next write. Agents such as the
// Cursor CLI ignore an Enter in the same write as the paste.
async function expectSubmitted(pty: FakePty | undefined, start: string): Promise<void> {
  await vi.waitFor(() => {
    const written = pty?.written ?? []
    const at = written.findIndex((data) => data.startsWith(`\x1b[200~${start}`))
    expect(at).toBeGreaterThanOrEqual(0)
    expect(written[at]?.endsWith('\x1b[201~')).toBe(true)
    expect(written[at + 1]).toBe('\r')
  })
}

// A task of a pipeline that starts with Implement, ready and in its workspace, so Implement runs.
async function implementing(): Promise<Task> {
  const task = await created({ title: 'Export', agent: 'claude-code' })
  tasks.applyEvent(task.id, {
    source: 'human',
    type: 'state.overridden',
    payload: { to: 'ready', stage: 'implement' },
  })
  return withWorkspace(task.id)
}

// The default pipeline with a person's review in place of the agent's, for the cases about it.
function withManualReview(): void {
  const manual: Stage = { id: 'review', kind: 'review', gate: 'manual' }
  settings = {
    ...DEFAULT_PROJECT_SETTINGS,
    stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
      step.id === 'agent-review' ? manual : step,
    ),
  }
}

// Test spec starts Codex with its own words, beside Claude Code in Shell 1.
function withOwnAgent(): void {
  const own = (stage: Stage) =>
    stage.id === 'test-spec' ? { ...stage, agent: 'codex', args: '--model gpt-5' } : stage
  settings = {
    ...DEFAULT_PROJECT_SETTINGS,
    stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
      'parallel' in step ? { ...step, parallel: step.parallel.map(own) } : step,
    ),
  }
}

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
    const skill = join(repo.root, 'home', '.claude', 'skills', 'linear-issue')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: linear-issue\n---\n')
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

  it('writes no Attachments section for a task without attachments', async () => {
    const task = await created({ title: 'Export tasks' })
    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context).toBe('# Export tasks\n\n(No notes.)\n')
  })
})

describe('attachments', () => {
  const files = () =>
    decodeAttachments([
      { name: 'login error.png', data: Buffer.from('image').toString('base64') },
      { name: 'spec.docx', data: Buffer.from('word').toString('base64') },
    ])
  const listed = [
    '- `.natamux-task/attachments/login-error.png` (5 B)',
    '- `.natamux-task/attachments/spec.docx` (4 B)',
  ]

  async function createdWithFiles(input: Record<string, unknown>): Promise<Task> {
    const task = tasks.createTask(input, files())
    await runner.settle()
    return tasks.getTask(task.id)
  }

  it('lists the attachments in the context of a typed task', async () => {
    const task = await createdWithFiles({ title: 'Export tasks', description: 'As CSV.' })

    expect(task).toMatchObject({ stage: 'spec', state: 'ready' })
    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context.startsWith('# Export tasks\n\nAs CSV.\n\n## Attachments\n')).toBe(true)
    for (const line of listed) {
      expect(context).toContain(line)
    }
  })

  it('lists the attachments in the context read from a GitHub issue', async () => {
    const task = await createdWithFiles({
      title: 'Export',
      context: { source: 'github-issue', ref: '#7' },
    })

    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context).toContain('Dates in UTC, please.')
    expect(context).toContain('## Attachments')
    expect(context).toContain(listed[0])
  })

  it('puts the attachments into the worktree, where the agent finds them', async () => {
    const task = await withWorkspace((await createdWithFiles({ title: 'Export' })).id)

    const dir = join(task.workspace ?? '', STAGE_DIR, 'attachments')
    expect(readFileSync(join(dir, 'login-error.png'), 'utf8')).toBe('image')
    expect(readFileSync(join(dir, 'spec.docx'), 'utf8')).toBe('word')
    expect(runGit(task.workspace ?? '', 'status', '--porcelain')).toBe('')
  })

  it('names the attachments to a context agent, and adds them when it leaves them out', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      context: { sources: [{ type: 'skill', name: 'Jira', skill: 'jira-ticket' }] },
    }
    const waiting = await createdWithFiles({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', name: 'Jira', ref: 'ABC-1' },
    })
    const task = await withWorkspace(waiting.id)
    expect(launchScript(task.id)).toContain('.natamux-task/attachments/login-error.png (5 B)')
    expect(launchScript(task.id)).toContain('list them under an Attachments heading')

    writeWorking(task, 'context.md', '# ABC-1\n\nExport as CSV.\n')
    await runner.act(task.id, 'report', { terminal: '1' })

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'context', state: 'review' })
    const context = readFileSync(join(task.workspace ?? '', STAGE_DIR, 'context.md'), 'utf8')
    expect(context.startsWith('# ABC-1\n\nExport as CSV.\n\n## Attachments\n')).toBe(true)
    expect(context).toContain(listed[1])
  })

  it('keeps the Attachments section an agent wrote itself', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      context: { sources: [{ type: 'skill', name: 'Jira', skill: 'jira-ticket' }] },
    }
    const waiting = await createdWithFiles({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', name: 'Jira', ref: 'ABC-1' },
    })
    const task = await withWorkspace(waiting.id)
    const written = '# ABC-1\n\n## Attachments\n\n- the screenshot and the spec\n'
    writeWorking(task, 'context.md', written)
    await runner.act(task.id, 'report', { terminal: '1' })

    expect(readFileSync(join(task.workspace ?? '', STAGE_DIR, 'context.md'), 'utf8')).toBe(written)
  })
})

describe('skills in stage prompts', () => {
  it("calls a skill the agent's way, and names a skill that is missing on the stage", async () => {
    const skill = join(repo.root, 'home', '.claude', 'skills', 'review-pr')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: review-pr\ndescription: Reviews\n---\n')
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'spec'
          ? { ...step, prompt: 'Use {skill:review-pr}, then {skill:gone}.' }
          : step,
      ),
    }
    const waiting = await created({ title: 'Export tasks', agent: 'claude-code' })
    const task = await withWorkspace(waiting.id)

    expect(launchScript(task.id)).toContain('Use /review-pr, then the gone skill.')
    const started = tasks
      .listEvents(task.id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.started' && event.payload.stage === 'spec')
    expect(started.at(-1)?.payload).toMatchObject({ missingSkills: ['gone'], agent: 'claude-code' })
  })

  it('gives an agent without the skill its file to read', async () => {
    const skill = join(repo.root, 'home', '.claude', 'skills', 'review-pr')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: review-pr\n---\n')
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'spec' ? { ...step, prompt: 'Use {skill:review-pr}.' } : step,
      ),
    }
    const waiting = await created({ title: 'Export tasks', agent: 'codex' })
    const task = await withWorkspace(waiting.id)

    expect(launchScript(task.id)).toContain(
      `Use the skill in ${join(skill, 'SKILL.md')} (read that file and follow it).`,
    )
  })
})

describe('settings from the Stages screen', () => {
  const withSpec = (change: Partial<Stage>) => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'spec' ? { ...step, ...change } : step,
      ),
    }
  }

  it('moves on from a stage that does not wait for approval', async () => {
    withSpec({ gate: 'auto' })
    const waiting = await created({ title: 'Export tasks', agent: 'claude-code' })
    const task = await withWorkspace(waiting.id)
    writeWorking(
      task,
      'spec.md',
      '# Goal\nx\n# Scope\nx\n# Out of scope\nx\n# Acceptance criteria\nx\n# Code map\nx\n',
    )

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('plan')
  })

  it('fills the placeholders of a prompt of your own, and adds the done line once', async () => {
    withSpec({ prompt: 'Spec {title} | {description} | {dir} | {ref}.' })
    const waiting = await created({
      title: 'Export tasks',
      description: 'CSV please',
      agent: 'claude-code',
      context: { source: 'manual', ref: '#9' },
    })
    const task = await withWorkspace(waiting.id)

    const script = launchScript(task.id)
    expect(script).toContain('Spec Export tasks | CSV please | .natamux-task | #9.')
    expect(script.split('natamux done').length - 1).toBe(1)
    expect(script).not.toContain('Do not commit')
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

  it('gives an extra shell no stage, so an agent started there by hand leaves the task alone', async () => {
    const task = await withWorkspace((await atSpec()).id)
    expect(runner.laneFor(task, '1')).toEqual({ stage: 'spec', state: 'running' })
    expect(runner.laneFor(task, undefined)).toEqual({ stage: 'spec', state: 'running' })
    expect(runner.laneFor(task, '2')).toBeNull()
  })

  it('sends failed checks back to the agent once, then waits for you', async () => {
    const task = await withWorkspace((await atSpec()).id)

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).state).toBe('running')
    expect(ptys[0]?.written.join('')).toContain('spec.md is missing')
    await expectSubmitted(ptys[0], 'The checks of this stage failed')

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).state).toBe('needs-input')
  })

  it('waits for approval when the output has its sections, and drops the turn end after', async () => {
    const task = await withWorkspace((await atSpec()).id)
    writeWorking(
      task,
      'spec.md',
      '# Goal\nx\n# Scope\nx\n# Out of scope\nx\n# Acceptance criteria\nx\n# Code map\nx\n',
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
      'spec.md has no Scope, Out of scope, Acceptance criteria, Code map section',
    ])
  })
})

describe('an agent review', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        {
          id: 'agent-review',
          kind: 'agent',
          gate: 'auto',
          output: 'review.md',
          sections: ['Findings'],
          ownShell: true,
        },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  })

  async function atReview(): Promise<Task> {
    const task = await implementing()
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    return tasks.getTask(task.id)
  }

  it('starts its own agent in Shell 5 when Implement is done', async () => {
    const task = await atReview()

    expect(task).toMatchObject({ stage: 'agent-review', state: 'running' })
    expect(launchScript(task.id, '5')).toContain('review.md')
    expect(launchScript(task.id, '5')).toContain('natamux stage back')
  })

  it('sends the task back to Implement with its findings, twice, then waits for you', async () => {
    const task = await atReview()

    await runner.act(task.id, 'reopen', { terminal: '5', reason: 'Fix the dates' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(ptys[0]?.written.join('')).toContain('Fix the dates')
    // The turn of the reviewer ends after it sent the task back, as after natamux done.
    expect(runner.takeReport(task.id, '5')).toBe(true)

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('agent-review')
    await runner.act(task.id, 'reopen', { terminal: '5', reason: 'Still wrong' })
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    await expect(runner.act(task.id, 'reopen', { terminal: '5', reason: 'Again' })).rejects.toThrow(
      /waits for the user/,
    )
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'needs-input' })
  })

  it('lets a person send it back any number of times', async () => {
    const task = await atReview()
    for (const reason of ['one', 'two', 'three']) {
      await runner.act(task.id, 'reopen', { reason })
      await runner.act(task.id, 'report', { terminal: '1' })
      await runner.settle()
    }
    expect(tasks.getTask(task.id).stage).toBe('agent-review')
  })
})

describe('a check on CI', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [{ id: 'implement', kind: 'agent', gate: 'manual', ci: true }],
    }
  })

  async function reported(ci: boolean): Promise<Task> {
    const task = await implementing()
    if (ci) {
      tasks.setCi(task.id, 'implement', true)
    }
    await runner.act(task.id, 'report', { terminal: '1' })
    return task
  }

  const lastOf = (id: string, type: EventType) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === type)
      .at(-1)?.event

  it('waits for CI after the local checks, then passes the stage', async () => {
    const task = await reported(true)

    expect(ciCalls).toEqual([{ worktree: task.workspace, branch: task.branch }])
    expect(lastOf(task.id, 'stage.ci-started')?.payload).toMatchObject({
      stage: 'implement',
      branch: `natamux-ci/${task.branch}`,
    })
    expect(tasks.getTask(task.id).state).toBe('running')

    finishCi({ passed: true, reasons: [], passes: ['CI passed: CI u'], final: false })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('review')
    expect(lastOf(task.id, 'stage.checked')?.payload.passes).toContain('CI passed: CI u')
  })

  it('gives a CI failure to the agent once', async () => {
    const task = await reported(true)

    finishCi({ passed: false, reasons: ['CI E2E failure: u\nError'], passes: [], final: false })
    await runner.settle()

    expect(lastOf(task.id, 'stage.check-failed')?.payload).toMatchObject({ final: false })
    await expectSubmitted(ptys[0], 'The checks of this stage failed')
    expect(ptys[0]?.written.join('')).toContain('CI E2E failure')
  })

  it('waits for you when the agent cannot fix it', async () => {
    const task = await reported(true)

    finishCi({ passed: false, reasons: ['No CI run started'], passes: [], final: true })
    await runner.settle()

    expect(lastOf(task.id, 'stage.check-failed')?.payload).toMatchObject({ final: true })
    expect(tasks.getTask(task.id).state).toBe('needs-input')
  })

  it('goes on with a CI check after a restart, and runs one at a time', async () => {
    const task = await reported(true)
    await runner.act(task.id, 'report', { terminal: '1' })
    expect(ciCalls).toHaveLength(1)

    runner.stop()
    runner = makeRunner(agents)
    runner.start()
    expect(ciCalls).toHaveLength(2)
    finishCi({ passed: true, reasons: [], passes: ['CI passed: CI u'], final: false })
    await runner.settle()
    expect(tasks.getTask(task.id).state).toBe('review')
  })

  it('does not wait for CI when the task did not ask for it', async () => {
    const task = await reported(false)

    expect(ciCalls).toEqual([])
    expect(tasks.getTask(task.id).state).toBe('review')
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
    // An agent the user started in another shell works on no stage of the group.
    expect(runner.laneFor(task, '3')).toBeNull()
  })

  it('starts the agent and the words of a stage in its own shell, and keeps the task agent', async () => {
    withOwnAgent()
    const task = await atBuild()

    expect(launchScript(task.id)).toMatch(/^claude /)
    expect(launchScript(task.id, '2')).toMatch(/^codex /)
    expect(launchScript(task.id, '2')).toContain('--model gpt-5')
    expect(tasks.getTask(task.id).agent).toBe('claude-code')
    const launches = tasks
      .listEvents(task.id)
      .filter((entry) => entry.event.type === 'agent.launched')
      .map((entry) => entry.event.payload)
    expect(launches).toContainEqual(expect.objectContaining({ agent: 'codex', stage: 'test-spec' }))
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

describe('a stage that runs as a workflow', () => {
  // Plan waits for approval; the next step is Test spec and Implement.
  async function atPlanReview(agent = 'claude-code'): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'plan' },
    })
    return withWorkspace(task.id)
  }

  const lastStart = (id: string, stage: string) =>
    tasks
      .listEvents(id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.started' && event.payload.stage === stage)
      .at(-1)

  it('asks the agent for a workflow when Implement starts after Plan is approved', async () => {
    const task = await atPlanReview()
    tasks.setWorkflow(task.id, 'implement', true)

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'running', implement: 'running' })
    expect(launchScript(task.id).split(WORKFLOW_LINE)).toHaveLength(2)
    expect(launchScript(task.id, '2')).not.toContain(WORKFLOW_LINE)
    expect(lastStart(task.id, 'implement')?.payload.workflow).toBe(true)
    expect(lastStart(task.id, 'test-spec')?.payload.workflow).toBeUndefined()
  })

  it('sends the same prompt without the workflow when it is off', async () => {
    const task = await atPlanReview()

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(launchScript(task.id)).toContain('Write the tests first')
    expect(launchScript(task.id)).not.toContain(WORKFLOW_LINE)
    expect(lastStart(task.id, 'implement')?.payload.workflow).toBeUndefined()
  })

  it('asks for it in the prompt pasted to an agent that runs already', async () => {
    const task = await atPlanReview()
    tasks.setWorkflow(task.id, 'implement', true)
    terminals.run(task.id, 'claude')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'

    await runner.act(task.id, 'approve')
    await runner.settle()

    const pasted = pty.written.join('')
    expect(pasted).toContain('\x1b[200~Task: Export tasks')
    expect(pasted.split(WORKFLOW_LINE)).toHaveLength(2)
  })

  it('asks for it again when the stage is sent back, but not with failed checks', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto', check: 'make test', workflow: true },
        { id: 'review', kind: 'review', gate: 'manual' },
      ],
    }
    runCommand = async () => ({ code: 1, output: 'tests failed' })
    const made = await created({ title: 'Export', agent: 'claude-code' })
    tasks.setWorkflow(made.id, 'implement', true)
    await withWorkspace(made.id)
    const task = await runner.act(made.id, 'start')
    const pty = ptys[0] as FakePty
    pty.program = 'node'

    await runner.act(task.id, 'report', { terminal: '1' })
    await expectSubmitted(pty, 'The checks of this stage failed')
    const failedPrompt = pty.written.find((data) => data.includes('The checks of this stage'))
    expect(failedPrompt).not.toContain(WORKFLOW_LINE)

    await runner.act(task.id, 'reopen', { to: 'implement', reason: 'Fix the dates' })
    await expectSubmitted(pty, 'Task: Export')
    const again = pty.written.filter((data) => data.startsWith('\x1b[200~Task: Export')).at(-1)
    expect(again).toContain(WORKFLOW_LINE)
    expect(again).toContain('What to change: Fix the dates')
  })

  it('never asks an agent other than Claude Code for a workflow', async () => {
    const task = await atPlanReview('claude-code')
    tasks.setWorkflow(task.id, 'implement', true)
    tasks.recordAgentLaunch(task.id, 'codex')

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(launchScript(task.id)).not.toContain(WORKFLOW_LINE)
    expect(lastStart(task.id, 'implement')?.payload.workflow).toBeUndefined()
  })

  it('asks for it in Plan when only Plan is ticked, and not in Implement', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'spec' },
    })
    tasks.setWorkflow(task.id, 'plan', true)
    await withWorkspace(task.id)

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'plan', state: 'running' })
    expect(launchScript(task.id)).toContain('plan.md')
    expect(launchScript(task.id).split(WORKFLOW_LINE)).toHaveLength(2)
    expect(lastStart(task.id, 'plan')?.payload.workflow).toBe(true)
  })

  it('starts without the workflow when it was ticked and then unticked', async () => {
    const task = await atPlanReview()
    tasks.setWorkflow(task.id, 'implement', true)
    tasks.setWorkflow(task.id, 'implement', false)

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(launchScript(task.id)).toContain('Write the tests first')
    expect(launchScript(task.id)).not.toContain(WORKFLOW_LINE)
  })

  it('sends nothing to a running stage, and uses the choice at its next start', async () => {
    const task = await atPlanReview()
    await runner.act(task.id, 'approve')
    await runner.settle()
    const written = () => ptys.map((pty) => pty.written.join(''))
    const before = written()

    tasks.setWorkflow(task.id, 'implement', true)
    await runner.settle()

    expect(written()).toEqual(before)
    expect(tasks.getTask(task.id).lanes.implement).toBe('running')

    // Each send back pastes the prompt again, with the line once, not once per round.
    for (const reason of ['Fix the dates', 'Fix the times']) {
      await runner.act(task.id, 'reopen', { to: 'implement', reason })
      const sent = () =>
        ptys.flatMap((pty) => pty.written).filter((data) => data.includes(`change: ${reason}`))
      await vi.waitFor(() => expect(sent()).toHaveLength(1))
      expect(sent()[0]?.split(WORKFLOW_LINE)).toHaveLength(2)
    }
  })

  it('asks for it again once the task is back on Claude Code', async () => {
    const task = await atPlanReview()
    tasks.setWorkflow(task.id, 'implement', true)
    tasks.recordAgentLaunch(task.id, 'codex')
    tasks.recordAgentLaunch(task.id, 'claude-code')

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(launchScript(task.id).split(WORKFLOW_LINE)).toHaveLength(2)
  })

  it('drops the workflow when the settings no longer offer it for the stage', async () => {
    const task = await atPlanReview()
    tasks.setWorkflow(task.id, 'implement', true)
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'build' && 'parallel' in step
          ? {
              ...step,
              parallel: step.parallel.map((stage) =>
                stage.id === 'implement' ? { ...stage, workflow: false } : stage,
              ),
            }
          : step,
      ),
    }

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(launchScript(task.id)).toContain('Write the tests first')
    expect(launchScript(task.id)).not.toContain(WORKFLOW_LINE)
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
    await expectSubmitted(pty, 'Task: Export tasks')
  })

  it('starts the agent again when it ended and only the shell is left', async () => {
    const { task, pty } = await atSpecWithShell('bash')
    clock += LAUNCH_GRACE_MS + 1

    await runner.act(task.id, 'reopen', { to: 'spec' })

    const typed = pty.written.join('')
    expect(typed.split(`agent-launch-${task.id}.sh`)).toHaveLength(3)
    expect(typed).not.toContain('\x1b[200~')
  })

  it('on Windows, starts the agent again only once the shell says the agent ended', async () => {
    runner.stop()
    const agents = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: join(dataDir, 'settings.json'),
      platform: 'win32',
    })
    runner = new StageRunner({
      tasks,
      terminals,
      agents,
      bus,
      dataDir,
      repoPath: () => repo.dir,
      now: () => clock,
      platform: 'win32',
    })
    runner.start()
    const exited = "$Host.UI.RawUI.WindowTitle = 'natamux-agent-exited'"
    const { task, pty } = await atSpecWithShell('powershell.exe')
    clock += LAUNCH_GRACE_MS + 1
    expect(pty.written.join('').split(exited)).toHaveLength(2)

    // The shell echoes the typed line, which is no sign that the agent ended.
    runner.onOutput(task.id, '1', exited)
    await runner.act(task.id, 'reopen', { to: 'spec' })
    expect(pty.written.join('')).toContain('\x1b[200~')

    runner.onOutput(task.id, '1', '\x1b]0;natamux-ag')
    runner.onOutput(task.id, '1', 'ent-exited\x07PS C:\\work> ')
    await runner.act(task.id, 'reopen', { to: 'spec' })
    // The launch line waits for the Enter of the prompt pasted before.
    await vi.waitFor(() => expect(pty.written.join('').split(exited)).toHaveLength(3))
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
    await expectSubmitted(pty, 'Also add a test')
  })

  it('ends the shells of a task that is cancelled', async () => {
    const { task, pty } = await atSpecWithShell('node')

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })

    expect(pty.killed).toBe(true)
    expect(terminals.list(task.id)).toEqual([])
  })
})

describe('a resume after a restart', () => {
  // The spec agent ended its turn with this payload, then the app restarted: the agent and its
  // shell are gone, the task still waits for you.
  async function waitingAfterRestart(
    payload: Record<string, unknown>,
    agent = 'claude-code',
  ): Promise<Task> {
    const task = await withWorkspace((await created({ title: 'Export tasks', agent })).id)
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', ...payload },
    })
    terminals.close(task.id)
    return tasks.getTask(task.id)
  }

  // As the Resume button sends it: no stage.
  async function resume(taskId: string, payload: Record<string, unknown> = {}): Promise<void> {
    tasks.applyEvent(taskId, { source: 'human', type: 'agent.resumed', payload })
    await runner.settle()
  }

  const typedLaunch = (id: string) =>
    ptys.at(-1)?.written.join('').includes(`agent-launch-${id}.sh`)

  // The build step, with the session each stage reported, after a restart.
  async function buildAfterRestart(): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    await withWorkspace(task.id)
    for (const [stage, sessionId] of [
      ['implement', 's-1'],
      ['test-spec', 's-2'],
    ]) {
      tasks.applyEvent(task.id, {
        source: 'agent',
        type: 'agent.finished',
        payload: { stage, sessionId },
      })
    }
    terminals.close(task.id)
    return tasks.getTask(task.id)
  }

  it('keeps the Claude Code session of a stage after another stage started its own agent', async () => {
    withOwnAgent()
    const task = await buildAfterRestart()
    await resume(task.id, { stage: 'test-spec' })
    expect(launchScript(task.id, '2')).toMatch(/^codex /)
    terminals.close(task.id)

    await resume(task.id, { stage: 'implement' })

    expect(launchScript(task.id)).toContain("--resume 's-1'")
  })

  it('goes on with the last Claude Code session of the stage, with no new prompt', async () => {
    const task = await waitingAfterRestart({ sessionId: 's-1' })
    expect(task.state).toBe('needs-input')

    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(typedLaunch(task.id)).toBe(true)
    expect(launchScript(task.id)).toContain("--resume 's-1'")
    expect(launchScript(task.id)).not.toContain('Task: Export tasks')
  })

  it('picks the newest session of the stage', async () => {
    const task = await waitingAfterRestart({ sessionId: 's-1' })
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.resumed',
      payload: { stage: 'spec', sessionId: 's-2' },
    })
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-2' },
    })

    await resume(task.id)

    expect(launchScript(task.id)).toContain("--resume 's-2'")
  })

  it('never takes a session of another task', async () => {
    const other = await waitingAfterRestart({ sessionId: 'other' })
    const task = await waitingAfterRestart({})

    await resume(task.id)

    expect(other.id).not.toBe(task.id)
    expect(launchScript(task.id)).not.toContain('--resume')
  })

  it('goes on with the session of a stage that waits for approval', async () => {
    const task = await waitingAfterRestart({ sessionId: 's-1' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'spec' },
    })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'review' })

    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(launchScript(task.id)).toContain("--resume 's-1'")
  })

  it.each([
    ['no session', {}],
    ['an empty session', { sessionId: '' }],
    ['a session that is not text', { sessionId: 42 }],
  ])('starts the agent with the stage prompt with %s', async (_name, payload) => {
    const task = await waitingAfterRestart(payload)

    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(typedLaunch(task.id)).toBe(true)
    expect(launchScript(task.id)).toContain('Task: Export tasks')
    expect(launchScript(task.id)).not.toContain('--resume')
  })

  const otherAgents = ['codex', 'gemini', 'copilot', 'cursor']

  // Only Claude Code goes on with a session.
  it.each(otherAgents)('starts %s with the stage prompt', async (agent) => {
    const task = await waitingAfterRestart({ sessionId: 's-1' }, agent)

    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(typedLaunch(task.id)).toBe(true)
    expect(launchScript(task.id)).toContain('Task: Export tasks')
    expect(launchScript(task.id)).not.toContain('--resume')
  })

  it.each(['claude-code', 'codex'])('types nothing when %s still runs', async (agent) => {
    const task = await withWorkspace((await created({ title: 'Export tasks', agent })).id)
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    const written = pty.written.length

    // Twice, as a quick double click sends it.
    await resume(task.id)
    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(ptys).toHaveLength(1)
    expect(pty.written).toHaveLength(written)
  })

  it('starts no agent for a prompt typed to the agent', async () => {
    const task = await waitingAfterRestart({ sessionId: 's-1' })

    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.resumed',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(terminals.list(task.id)).toEqual([])
  })

  it('goes on with the session of the stage, in the terminal of that stage', async () => {
    const task = await buildAfterRestart()

    await resume(task.id, { stage: 'test-spec' })

    expect(tasks.getTask(task.id).lanes).toEqual({
      'test-spec': 'running',
      implement: 'needs-input',
    })
    expect(terminals.list(task.id)).toEqual(['2'])
    expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
  })

  it('never takes the session of another stage of the group', async () => {
    const task = await buildAfterRestart()

    await resume(task.id, { stage: 'implement' })

    expect(terminals.list(task.id)).toEqual(['1'])
    expect(launchScript(task.id)).toContain("--resume 's-1'")
  })

  it('leaves the agent of the other terminal alone', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    await withWorkspace(task.id)
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'test-spec', sessionId: 's-2' },
    })
    // The agent of terminal 1 runs on; the one of terminal 2 is gone.
    const running = [...ptys]
    for (const pty of running) {
      pty.program = 'node'
    }
    const written = running.map((pty) => pty.written.length)
    terminals.closeOne(task.id, '2')

    await resume(task.id, { stage: 'test-spec' })

    expect(terminals.list(task.id)).toEqual(['1', '2'])
    expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
    expect(running.map((pty) => pty.written.length)).toEqual(written)
  })

  it('starts the stage prompt on a new start of the stage, even with a session', async () => {
    const task = await waitingAfterRestart({ sessionId: 's-1' })

    await runner.act(task.id, 'reopen', { to: 'spec' })
    await runner.settle()

    expect(typedLaunch(task.id)).toBe(true)
    expect(launchScript(task.id)).toContain('Task: Export tasks')
    expect(launchScript(task.id)).not.toContain('--resume')
  })

  it.each([
    ['Claude Code with its session', 'claude-code'],
    ['Codex with the stage prompt', 'codex'],
  ])('waits for you with the reason when %s cannot start', async (_name, agent) => {
    const task = await waitingAfterRestart({ sessionId: 's-1' }, agent)
    vi.spyOn(terminals, 'run').mockImplementation(() => {
      throw new Error('No shell')
    })

    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('needs-input')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'stage.check-failed',
      payload: { stage: 'spec', reasons: ['No shell'] },
    })
  })

  it('in a group, puts only the resumed stage back when its agent cannot start', async () => {
    const task = await buildAfterRestart()
    vi.spyOn(terminals, 'run').mockImplementation(() => {
      throw new Error('No shell')
    })

    await resume(task.id, { stage: 'test-spec' })

    expect(tasks.getTask(task.id).lanes).toEqual({
      'test-spec': 'needs-input',
      implement: 'needs-input',
    })
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'stage.check-failed',
      payload: { stage: 'test-spec', reasons: ['No shell'] },
    })
  })
})

describe('the agent in each terminal', () => {
  it('names the agent a person typed into an extra shell, not the agent of the task', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await withWorkspace(task.id)
    terminals.run(task.id, 'codex', '2')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    pty.commandLines = ['node /usr/local/bin/codex', '/usr/local/lib/codex/vendor/codex']

    expect(await runner.agentIn(task.id, '2')).toBe('codex')
  })

  it('names an agent typed by hand when the task has another agent', async () => {
    const task = await created({ title: 'Export tasks', agent: 'gemini' })
    await withWorkspace(task.id)
    terminals.run(task.id, 'claude', '2')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'claude'
    pty.commandLines = ['claude']

    expect(await runner.agentIn(task.id, '2')).toBe('claude-code')
    expect(tasks.getTask(task.id).agent).toBe('gemini')
  })

  it('names an agent that a wrapper such as npx starts', async () => {
    const task = await created({ title: 'Export tasks' })
    await withWorkspace(task.id)
    terminals.run(task.id, 'npx @openai/codex', '2')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    pty.commandLines = [
      'node /usr/local/bin/npx @openai/codex',
      'node /home/u/.npm/_npx/1a2b/node_modules/.bin/codex',
    ]

    expect(await runner.agentIn(task.id, '2')).toBe('codex')
  })

  it('follows the agents a person starts and quits in an extra shell', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await withWorkspace(task.id)
    terminals.run(task.id, 'claude', '2')
    const pty = ptys.at(-1) as FakePty

    pty.commandLines = ['claude']
    expect(await runner.agentIn(task.id, '2')).toBe('claude-code')
    pty.commandLines = []
    expect(await runner.agentIn(task.id, '2')).toBeNull()
    pty.commandLines = ['node /usr/local/bin/codex']
    expect(await runner.agentIn(task.id, '2')).toBe('codex')
    // The shell itself ended.
    terminals.closeOne(task.id, '2')
    expect(await runner.agentIn(task.id, '2')).toBeNull()
  })

  it('names no agent when the processes cannot be read', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await withWorkspace(task.id)
    terminals.run(task.id, 'codex', '2')
    const pty = ptys.at(-1) as FakePty
    pty.commands = async () => {
      throw new Error('ps failed')
    }

    expect(await runner.agentIn(task.id, '2')).toBeNull()
  })

  it('names no agent for a program that is not one', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await withWorkspace(task.id)
    terminals.run(task.id, 'vim notes.md', '2')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'vim'
    pty.commandLines = ['vim notes.md']

    expect(await runner.agentIn(task.id, '2')).toBeNull()
    expect(await runner.agentIn(task.id, '3')).toBeNull()
  })

  it('names the agent it started where the processes cannot be listed', async () => {
    const task = await created({ title: 'Export tasks' })
    // The new workspace starts the stage with the agent picked here, not the default one.
    await runner.startByHand(task.id, 'gemini', undefined, () => workspaces.create(task.id))
    await runner.settle()
    const pty = ptys[0] as FakePty
    pty.program = 'node'

    pty.commandLines = undefined
    expect(await runner.agentIn(task.id, '1')).toBe('gemini')
    // Just after the launch the agent may not be up yet.
    pty.commandLines = []
    expect(await runner.agentIn(task.id, '1')).toBe('gemini')
    // Later, the processes tell: the agent ended and another program runs.
    clock += LAUNCH_GRACE_MS + 1
    pty.commandLines = ['vim notes.md']
    expect(await runner.agentIn(task.id, '1')).toBeNull()
    pty.commandLines = ['codex']
    expect(await runner.agentIn(task.id, '1')).toBe('codex')
  })
})

describe('a send back', () => {
  beforeEach(withManualReview)

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

  it('starts the agent again when you resume the review and no agent runs', async () => {
    const task = await inReview()

    tasks.applyEvent(task.id, { source: 'human', type: 'agent.resumed' })
    await runner.settle()

    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    expect(typesSince(task.id, 'agent.resumed')).toEqual([
      'agent.resumed',
      'stage.started',
      'agent.launched',
    ])
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
