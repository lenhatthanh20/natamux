import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  DEFAULT_PROJECT_SETTINGS,
  type EventType,
  type ProjectSettings,
  type Stage,
  type StageBranch,
  type Step,
  type Task,
} from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest'
import {
  addBareOrigin,
  createTestRepo,
  runGit,
  type TestRepo,
} from '../../__tests__/git-repo.fixture.js'
import { type FakePty, fakeSpawner } from '../../__tests__/terminal-fake.fixture.js'
import { trustPaths } from '../../agents/agent-launch.js'
import { AgentRunner } from '../../agents/agent-runner.js'
import { EventBus } from '../../shared/event-bus.js'
import { decodeAttachments } from '../../tasks/attachments.js'
import { archiveDir, STAGE_DIR } from '../../tasks/task-files.js'
import { isTaskEnded } from '../../tasks/task-helpers.js'
import { TaskIndex } from '../../tasks/task-index.js'
import { TaskService } from '../../tasks/task-service.js'
import { readEndedShells } from '../../terminals/ended-shells.js'
import { OpenShells } from '../../terminals/open-shells.js'
import {
  type OpenShell,
  openShellsPath,
  readOpenShells,
  saveOpenShells,
} from '../../terminals/open-shells-file.js'
import { TerminalService } from '../../terminals/terminal-service.js'
import { WorkspaceService } from '../../workspaces/workspace-service.js'
import type { CiResult } from '../ci-check.js'
import { MAX_AGENT_SEND_BACKS, StageActionError, StageInputError } from '../stage-actions.js'
import type { RunCommand } from '../stage-checks.js'
import { GO_ON_PROMPT } from '../stage-launcher.js'
import { WORKFLOW_LINE } from '../stage-prompts.js'
import { StageRunner } from '../stage-runner.js'
import { LAUNCH_GRACE_MS } from '../stage-shells.js'

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
    dataDir,
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
    // Cursor and Copilot trust the workspace in their config, which must not be the real one.
    trustPaths: trustPaths({}, join(repo.root, 'home')),
  })
  ghCalls = []
  runner = makeRunner(agents)
  runner.start()
})

function makeRunner(agents: AgentRunner, openShells?: OpenShells): StageRunner {
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
    openShells,
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

// A paste reaches the shell later, once the shell is ready for it.
async function expectWritten(pty: FakePty | undefined, text: string): Promise<void> {
  await vi.waitFor(() => expect(pty?.written.join('')).toContain(text))
}

// A task of a pipeline that starts with Implement, ready and in its workspace, so Implement runs.
async function implementing(stage = 'implement'): Promise<Task> {
  const task = await created({ title: 'Export', agent: 'claude-code' })
  tasks.applyEvent(task.id, {
    source: 'human',
    type: 'state.overridden',
    payload: { to: 'ready', stage },
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

// A command runs the tests, not an agent.
function withTestCommand(): void {
  const command: Stage = { id: 'testing', kind: 'testing', gate: 'auto', command: 'make test' }
  settings = {
    ...DEFAULT_PROJECT_SETTINGS,
    stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) => (step.id === 'testing' ? command : step)),
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

  it('ends the context of a subtask with its parent', async () => {
    const parent = await created({ title: 'Export', description: 'Use CSV.' })
    const task = await created({ title: 'Part A', parentId: parent.id })

    const context = readFileSync(join(archiveDir(dataDir, task.id), 'context.md'), 'utf8')
    expect(context).toContain('## Parent task')
    expect(context).toContain(`${parent.id}: Export`)
    expect(context).toContain('Use CSV.')
    expect(context).toContain(`Stage files of the parent: ${archiveDir(dataDir, parent.id)}`)
    const own = readFileSync(join(archiveDir(dataDir, parent.id), 'context.md'), 'utf8')
    expect(own).not.toContain('## Parent task')
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

  it('runs a skill picked for the task, with no source in the settings', async () => {
    const skill = join(repo.root, 'home', '.claude', 'skills', 'jira-ticket')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: jira-ticket\n---\n')
    const waiting = await created({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' },
    })

    const task = await withWorkspace(waiting.id)
    expect(launchScript(task.id)).toContain('/jira-ticket ABC-1')
    expect(launchScript(task.id)).toContain('.natamux-task/context.md')
    writeWorking(task, 'context.md', '# ABC-1\n\nExport as CSV.\n')
    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'context', state: 'review' })
  })

  it('asks for the skill, then context.md, and runs the picked skill over a namesake', async () => {
    const skill = join(repo.root, 'home', '.claude', 'skills', 'jira-ticket')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: jira-ticket\n---\n')
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      context: { sources: [{ type: 'skill', name: 'jira-ticket', skill: 'other-skill' }] },
    }
    const waiting = await created({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' },
    })

    const script = launchScript((await withWorkspace(waiting.id)).id)
    const call = script.indexOf('/jira-ticket ABC-1')
    const write = script.indexOf('Write what you found to .natamux-task/context.md.')
    expect(call).toBeGreaterThanOrEqual(0)
    expect(write).toBeGreaterThan(call)
    expect(script).not.toContain('other-skill')
    expect(script).not.toContain('list them under an Attachments heading')
  })

  it('fails the check of a picked skill when the agent wrote no context.md', async () => {
    const waiting = await created({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' },
    })
    const task = await withWorkspace(waiting.id)
    await runner.act(task.id, 'report', { terminal: '1' })

    const failed = tasks
      .listEvents(task.id)
      .filter((entry) => entry.event.type === 'stage.check-failed')
      .at(-1)?.event
    expect(failed?.payload).toMatchObject({
      stage: 'context',
      reasons: expect.arrayContaining(['.natamux-task/context.md is missing']),
    })
    expect(tasks.getTask(task.id).state).not.toBe('review')
  })

  it('gives an agent without a sure call the file of the picked skill', async () => {
    const skill = join(repo.root, 'home', '.claude', 'skills', 'jira-ticket')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: jira-ticket\n---\n')
    const waiting = await created({
      title: 'Export',
      agent: 'codex',
      context: { source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' },
    })

    const script = launchScript((await withWorkspace(waiting.id)).id)
    expect(script).toContain(`Read ${join(skill, 'SKILL.md')} and follow it for ABC-1.`)
    expect(script).not.toContain('/jira-ticket ABC-1')
  })

  it('names a picked skill the agent does not have on the stage', async () => {
    const waiting = await created({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', skill: 'gone', ref: 'ABC-1' },
    })

    const task = await withWorkspace(waiting.id)
    const started = tasks
      .listEvents(task.id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.started' && event.payload.stage === 'context')
    expect(started.at(-1)?.payload).toMatchObject({ missingSkills: ['gone'], agent: 'claude-code' })
  })

  it('reads a picked skill from the main checkout when the worktree does not have it', async () => {
    // Not committed, so the new worktree does not have it.
    const skill = join(repo.dir, '.claude', 'skills', 'jira-ticket')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: jira-ticket\n---\n')
    const waiting = await created({
      title: 'Export',
      agent: 'claude-code',
      context: { source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' },
    })

    const task = await withWorkspace(waiting.id)
    expect(launchScript(task.id)).toContain(
      `Read ${join(skill, 'SKILL.md')} and follow it for ABC-1.`,
    )
    const started = tasks
      .listEvents(task.id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.started' && event.payload.stage === 'context')
    expect(started.at(-1)?.payload.missingSkills).toBeUndefined()
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

describe('stage files of a removed workspace', () => {
  it('puts the latest spec into the next workspace, not the last archived one', async () => {
    const task = await created({ title: 'Export tasks' })
    // Held for review, so no agent starts in the workspaces.
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'spec' },
    })
    writeFileSync(join(archiveDir(dataDir, task.id), 'spec.md'), 'spec v1\n')
    const first = await withWorkspace(task.id)
    writeFileSync(join(first.workspace ?? '', STAGE_DIR, 'spec.md'), 'spec v2\n')

    await workspaces.remove(task.id, { deleteBranch: false })
    await runner.settle()
    const next = await withWorkspace(task.id)

    expect(readFileSync(join(next.workspace ?? '', STAGE_DIR, 'spec.md'), 'utf8')).toBe('spec v2\n')
  })
})

describe('failed side work', () => {
  let error: MockInstance<typeof console.error>

  beforeEach(() => {
    error = vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    error.mockRestore()
  })

  it('logs a failed archive of the stage files, and still starts the next stage', async () => {
    const task = await implementing()
    // A folder with the name of a stage file cannot be copied.
    mkdirSync(join(task.workspace ?? '', STAGE_DIR, 'x.md'))

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    // Implement runs in the step "build" of the default pipeline. Testing comes after it.
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'running' })
    expect(error).toHaveBeenCalledWith(
      expect.stringContaining(`Could not archive the stage files of ${task.id}`),
    )
  })

  it('logs a failed restore of the stage files, and still restores the attachments and starts the stage', async () => {
    const files = decodeAttachments([
      { name: 'login.png', data: Buffer.from('image').toString('base64') },
    ])
    const task = tasks.createTask({ title: 'Export', agent: 'claude-code' }, files)
    await runner.settle()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'implement' },
    })
    mkdirSync(join(archiveDir(dataDir, task.id), 'x.md'))

    const after = await withWorkspace(task.id)

    expect(after).toMatchObject({ stage: 'build', state: 'running' })
    const attachment = join(after.workspace ?? '', STAGE_DIR, 'attachments', 'login.png')
    expect(existsSync(attachment)).toBe(true)
    expect(error).toHaveBeenCalledWith(
      expect.stringContaining(`Could not restore the stage files of ${task.id}`),
    )
  })

  it('logs a failed delete of the CI branch of a finished task', async () => {
    // The test repository has no origin, so the push fails.
    const task = await implementing()
    tasks.setCi(task.id, 'testing', true)

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })
    await runner.settle()

    const ref = `natamux-ci/${task.branch}`
    expect(error).toHaveBeenCalledWith(
      expect.stringContaining(`Could not delete the CI branch ${ref} of ${task.id}`),
    )
  })

  it('logs stages that fail to start', async () => {
    const task = await implementing()
    vi.spyOn(runner, 'startLanes').mockRejectedValue(new Error('disk full'))

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    const logged = `Could not start the stages of ${task.id}: Error: disk full`
    expect(error).toHaveBeenCalledWith(logged)
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

  it('reads a plugin that only the untracked settings of the main checkout turn on', async () => {
    const home = join(repo.root, 'home')
    const install = join(home, '.claude', 'plugins', 'cache', 'm', 'acme', '1.0')
    const skill = join(install, 'skills', 'worktree')
    mkdirSync(skill, { recursive: true })
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: worktree\n---\n')
    writeFileSync(
      join(home, '.claude', 'plugins', 'installed_plugins.json'),
      JSON.stringify({
        plugins: { 'acme@m': [{ scope: 'local', projectPath: repo.dir, installPath: install }] },
      }),
    )
    mkdirSync(join(repo.dir, '.claude'), { recursive: true })
    writeFileSync(
      join(repo.dir, '.claude', 'settings.local.json'),
      JSON.stringify({ enabledPlugins: { 'acme@m': true } }),
    )
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'spec' ? { ...step, prompt: 'Use {skill:acme:worktree}.' } : step,
      ),
    }
    const waiting = await created({ title: 'Export tasks', agent: 'claude-code' })
    const task = await withWorkspace(waiting.id)

    expect(launchScript(task.id)).toContain(
      `Use the skill in ${join(skill, 'SKILL.md')} (read that file and follow it).`,
    )
    const started = tasks
      .listEvents(task.id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.started' && event.payload.stage === 'spec')
    expect(started.at(-1)?.payload.missingSkills).toBeUndefined()
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

  // The agent runs `natamux done` while a person clicks Mark as done.
  it('runs the checks once for two reports at the same time, and keeps the agent its try', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [{ id: 'implement', kind: 'agent', gate: 'manual', check: 'make test' }],
    }
    const calls: string[] = []
    let finish: () => void = () => undefined
    runCommand = (command) => {
      calls.push(command)
      return new Promise((resolve) => {
        finish = () => resolve({ code: 1, output: 'tests failed' })
      })
    }
    const task = await implementing()

    const reports = Promise.all([
      runner.act(task.id, 'report', { terminal: '1' }),
      runner.act(task.id, 'report'),
    ])
    await vi.waitFor(() => expect(calls).toHaveLength(1))
    finish()
    await reports

    const failures = tasks
      .listEvents(task.id)
      .filter(({ event }) => event.type === 'stage.check-failed')
    expect(failures.map(({ event }) => event.payload.final)).toEqual([false])
    expect(tasks.getTask(task.id).state).toBe('running')
    await expectSubmitted(ptys[0], 'The checks of this stage failed')

    // Once the first check ended, a report checks again.
    const again = runner.act(task.id, 'report', { terminal: '1' })
    await vi.waitFor(() => expect(calls).toHaveLength(2))
    finish()
    await again
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

  const reportsOf = (id: string, stage: string) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === 'stage.reported')
      .filter((entry) => entry.event.payload.stage === stage)

  it('refuses natamux done from the reviewer after it sent the task back', async () => {
    const task = await atReview()
    await runner.act(task.id, 'reopen', { terminal: '5', reason: 'Fix the dates' })
    await runner.settle()

    await expect(runner.act(task.id, 'report', { terminal: '5' })).rejects.toThrow(
      /agent-review stage of Terminal 5 does not run now/,
    )
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(reportsOf(task.id, 'implement')).toHaveLength(1)

    // The Implement agent still reports its own stage.
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
  })

  it('refuses natamux done from Shell 1 while the reviewer runs in its own shell', async () => {
    const task = await atReview()

    await expect(runner.act(task.id, 'report', { terminal: '1' })).rejects.toThrow(
      /No stage of Terminal 1 runs now/,
    )
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
    expect(reportsOf(task.id, 'agent-review')).toHaveLength(0)
  })

  it('takes a report of the stage named, from any shell', async () => {
    const task = await atReview()
    await runner.act(task.id, 'reopen', { terminal: '5', reason: 'Fix the dates' })
    await runner.settle()

    await runner.act(task.id, 'report', { terminal: '5', stage: 'implement' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
  })

  it('refuses the other actions from the reviewer after its send back, unless they name the stage', async () => {
    const task = await atReview()
    await runner.act(task.id, 'reopen', { terminal: '5', reason: 'Fix the dates' })
    await runner.settle()

    for (const action of ['approve', 'skip', 'pass', 'fail'] as const) {
      await expect(runner.act(task.id, action, { terminal: '5' })).rejects.toThrow(
        /agent-review stage of Terminal 5 does not run now/,
      )
    }
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })

    const failed = await runner.act(task.id, 'fail', {
      terminal: '5',
      stage: 'implement',
      reason: 'Stuck',
    })
    expect(failed).toMatchObject({ stage: 'implement', state: 'failed' })
  })

  it('takes a report from a shell that no stage uses, as before', async () => {
    const task = await atReview()

    await runner.act(task.id, 'report', { terminal: '3' })
    await runner.settle()

    // Reported and checked: review.md is missing, so the stage runs on.
    expect(reportsOf(task.id, 'agent-review')).toHaveLength(1)
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
  })
})

describe('a report from the shell of a stage of another step', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        {
          id: 'build',
          parallel: [
            {
              id: 'test-spec',
              kind: 'agent',
              gate: 'auto',
              output: 'test-spec.md',
              sections: ['Cases'],
            },
            { id: 'implement', kind: 'agent', gate: 'auto' },
          ],
        },
        {
          id: 'agent-review',
          kind: 'agent',
          gate: 'auto',
          output: 'review.md',
          sections: ['Findings'],
          ownShell: true,
          agent: 'cursor',
        },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  })

  // Test spec ran in Shell 2 and Implement in Shell 1; the reviewer runs in Shell 8.
  async function atReview(): Promise<Task> {
    const made = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(made.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    const task = await withWorkspace(made.id)
    writeWorking(task, 'test-spec.md', '# Cases\n- x\n')
    await runner.act(task.id, 'report', { terminal: '2' })
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
    return tasks.getTask(task.id)
  }

  it('keeps Implement running when the reviewer reports after its send back', async () => {
    const task = await atReview()
    await runner.act(task.id, 'reopen', { terminal: '8', to: 'implement', reason: 'Fix it' })
    await runner.settle()
    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    const reports = () =>
      tasks.listEvents(task.id).filter((entry) => entry.event.type === 'stage.reported')
    const before = reports()

    await expect(runner.act(task.id, 'report', { terminal: '8' })).rejects.toThrow(
      /agent-review stage of Terminal 8 does not run now/,
    )

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'build', state: 'running' })
    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    expect(reports()).toEqual(before)

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
  })
})

describe('a stage of your own with its own agent', () => {
  const docs: Stage = { id: 'write-docs', kind: 'agent', gate: 'auto', agent: 'codex' }

  function withStages(...own: Stage[]): void {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        ...own,
        { id: 'finish', kind: 'agent', gate: 'auto' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  }

  // Implement ran in Shell 1 with the task's agent and is done, so the stage of your own starts.
  async function afterImplement(): Promise<Task> {
    const task = await implementing()
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    return tasks.getTask(task.id)
  }

  const launches = (id: string) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === 'agent.launched')
      .map((entry) => entry.event.payload)
  const launchesOf = (id: string, agent: string) =>
    launches(id).filter((payload) => payload.agent === agent)

  it('starts its own agent in Shell 7, with its words, and keeps the task agent in Shell 1', async () => {
    withStages({ ...docs, args: '--model gpt-5' })
    const task = await afterImplement()

    expect(task).toMatchObject({ stage: 'write-docs', state: 'running', agent: 'claude-code' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(launchScript(task.id)).toMatch(/^claude /)
    expect(launchScript(task.id, '7')).toMatch(/^codex /)
    expect(launchScript(task.id, '7')).toContain('--model gpt-5')
    expect(launches(task.id).at(-1)).toMatchObject({ agent: 'codex', stage: 'write-docs' })
    expect(runner.laneFor(task, '7')).toEqual({ stage: 'write-docs', state: 'running' })
    expect(runner.laneFor(task, '1')).toBeNull()
  })

  it('gives the next stage to the same task agent in Shell 1, with no new launch', async () => {
    withStages(docs)
    const task = await afterImplement()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'finish', state: 'running' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    await expectSubmitted(ptys[0], 'Task: Export')
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)
  })

  it('gives its prompt to its own agent when the stage runs again', async () => {
    withStages(docs)
    const task = await afterImplement()
    const shell = ptys.at(-1)
    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    await runner.act(task.id, 'reopen', { to: 'write-docs' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'running' })
    expect(shell?.killed).toBe(false)
    await expectSubmitted(shell, 'Task: Export')
    expect(launchesOf(task.id, 'codex')).toHaveLength(1)
  })

  it('runs two stages with other agents in two shells, and closes neither', async () => {
    withStages(docs, { id: 'check-docs', kind: 'agent', gate: 'auto', agent: 'cursor' })
    const task = await afterImplement()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'check-docs', state: 'running' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(launchScript(task.id, '7')).toMatch(/^codex /)
    expect(launches(task.id).at(-1)).toMatchObject({ agent: 'cursor', stage: 'check-docs' })
    expect(runner.laneFor(tasks.getTask(task.id), '8')).toEqual({
      stage: 'check-docs',
      state: 'running',
    })
  })

  it('starts its own agent again in its shell when the daemon starts', async () => {
    withStages(docs)
    const task = await afterImplement()
    terminals.close(task.id)

    runner.resumeAgents()
    await runner.settle()

    const codex = launchesOf(task.id, 'codex')
    expect(codex).toHaveLength(2)
    expect(codex.at(-1)).toMatchObject({ stage: 'write-docs' })
    expect(launchScript(task.id, '7')).toMatch(/^codex /)
    expect(tasks.getTask(task.id).state).toBe('running')
  })

  it('keeps the Claude Code session of Shell 1 across a stage with its own agent', async () => {
    withStages(docs)
    const task = await implementing()
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.waiting',
      payload: { stage: 'implement', sessionId: 's-1' },
    })
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('write-docs')

    // Back to Implement: Claude Code in Shell 1 still runs and gets the prompt.
    await runner.act(task.id, 'reopen', { to: 'implement' })
    await runner.settle()
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)

    // After a restart it goes on with its session; the agent of Write docs does not count.
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'implement' },
    })
    terminals.close(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'agent.resumed', payload: {} })
    await runner.settle()

    expect(launchesOf(task.id, 'claude-code')).toHaveLength(2)
    expect(launchScript(task.id)).toContain("--resume 's-1'")
  })

  it('asks no agent but Claude Code for a workflow', async () => {
    withStages({ ...docs, workflow: true })
    const task = await implementing()
    tasks.setWorkflow(task.id, 'write-docs', true)

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(launchScript(task.id, '7')).toMatch(/^codex /)
    expect(launchScript(task.id, '7')).not.toContain(WORKFLOW_LINE)
  })
})

describe('the pipeline of the issue: the task agent in Shell 1, other agents beside it', () => {
  const copilot = { agent: 'copilot' }
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'spec', kind: 'agent', gate: 'auto' },
        {
          id: 'test-spec',
          kind: 'agent',
          gate: 'auto',
          output: 'test-spec.md',
          ownShell: true,
          ...copilot,
        },
        { id: 'implement', kind: 'agent', gate: 'auto' },
        {
          id: 'agent-review',
          kind: 'agent',
          gate: 'auto',
          output: 'review.md',
          ownShell: true,
          agent: 'cursor',
        },
        { id: 'ci-watch', kind: 'agent', gate: 'auto', ...copilot },
        { id: 'merge-pr', kind: 'agent', gate: 'auto', ...copilot },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  })

  const launchesOf = (id: string, agent: string) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === 'agent.launched')
      .map((entry) => entry.event.payload)
      .filter((payload) => payload.agent === agent)

  async function report(task: Task, terminal: string): Promise<Task> {
    await runner.act(task.id, 'report', { terminal })
    await runner.settle()
    return tasks.getTask(task.id)
  }

  // Spec and Test spec are done; Implement runs in Shell 1, Review is next.
  async function atImplement(): Promise<Task> {
    const task = await implementing('spec')
    await report(task, '1')
    writeWorking(task, 'test-spec.md', '# Cases\nx\n')
    return report(task, '7')
  }

  async function atReview(): Promise<Task> {
    const task = await report(await atImplement(), '1')
    writeWorking(task, 'review.md', '# Findings\nNone\n')
    return task
  }

  it('runs Spec and Implement in Shell 1, the others in Shells 7 to 11, and closes no agent', async () => {
    const task = await implementing('spec')
    expect(launchScript(task.id)).toMatch(/^claude /)

    let current = await report(task, '1')
    expect(current).toMatchObject({ stage: 'test-spec', state: 'running' })
    expect(launchScript(task.id, '7')).toMatch(/^copilot /)
    writeWorking(task, 'test-spec.md', '# Cases\nx\n')

    current = await report(task, '7')
    expect(current).toMatchObject({ stage: 'implement', state: 'running' })
    await expectSubmitted(ptys[0], 'Task: Export')

    current = await report(task, '1')
    expect(current).toMatchObject({ stage: 'agent-review', state: 'running' })
    expect(launchesOf(task.id, 'cursor').at(-1)).toMatchObject({ stage: 'agent-review' })
    expect(runner.laneFor(current, '9')).toEqual({ stage: 'agent-review', state: 'running' })
    writeWorking(task, 'review.md', '# Findings\nNone\n')

    current = await report(task, '9')
    expect(current).toMatchObject({ stage: 'ci-watch', state: 'running' })
    expect(launchScript(task.id, '10')).toMatch(/^copilot /)

    current = await report(task, '10')
    expect(current).toMatchObject({ stage: 'merge-pr', state: 'running' })
    expect(launchScript(task.id, '11')).toMatch(/^copilot /)
    expect(runner.laneFor(current, '11')).toEqual({ stage: 'merge-pr', state: 'running' })

    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)
    expect(launchesOf(task.id, 'copilot').map((payload) => payload.stage)).toEqual([
      'test-spec',
      'ci-watch',
      'merge-pr',
    ])
  })

  it('gives a send back from Review to Claude Code in Shell 1, then Review to its own agent', async () => {
    const task = await atReview()
    const review = ptys.at(-1)

    await runner.act(task.id, 'reopen', { terminal: '9', reason: 'Fix the dates' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    await expectWritten(ptys[0], 'What to change: Fix the dates')
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)

    await report(task, '1')
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'agent-review', state: 'running' })
    await expectSubmitted(review, 'Task: Export')
    expect(launchesOf(task.id, 'cursor')).toHaveLength(1)
    expect(ptys.some((pty) => pty.killed)).toBe(false)
  })

  it('gives a send back from CI/CD watch to Claude Code, then CI/CD watch to its own agent', async () => {
    const task = await report(await atReview(), '9')
    expect(task.stage).toBe('ci-watch')
    const watch = ptys.at(-1)

    // Without `to` a send back goes to the step before, which is Review here.
    await runner.act(task.id, 'reopen', { terminal: '10', to: 'implement', reason: 'CI failed' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    await expectWritten(ptys[0], 'What to change: CI failed')

    // Review runs again in between; its agent in Shell 9 gets the prompt.
    await report(task, '1')
    writeWorking(task, 'review.md', '# Findings\nNone\n')
    await report(task, '9')
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'ci-watch', state: 'running' })
    await expectSubmitted(watch, 'Task: Export')
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)
    expect(launchesOf(task.id, 'copilot').map((payload) => payload.stage)).toEqual([
      'test-spec',
      'ci-watch',
    ])
    expect(ptys.some((pty) => pty.killed)).toBe(false)
  })

  it('keeps Shell 5 for a stage in its own shell with no agent of its own', async () => {
    settings = {
      ...settings,
      stages: settings.stages.flatMap((step) =>
        step.id === 'agent-review'
          ? [
              {
                id: 'security-review',
                kind: 'agent',
                gate: 'auto',
                output: 'security.md',
                ownShell: true,
              } as const,
              step,
            ]
          : [step],
      ),
    }
    const task = await report(await atImplement(), '1')

    expect(task).toMatchObject({ stage: 'security-review', state: 'running' })
    expect(launchScript(task.id, '5')).toMatch(/^claude /)
    writeWorking(task, 'security.md', '# Findings\nNone\n')

    const next = await report(task, '5')
    expect(next).toMatchObject({ stage: 'agent-review', state: 'running' })
    expect(launchesOf(task.id, 'cursor').at(-1)).toMatchObject({ stage: 'agent-review' })
    expect(runner.laneFor(next, '10')).toEqual({ stage: 'agent-review', state: 'running' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
  })

  // The settings of a stage, changed while the task runs.
  function changeStage(id: string, change: Partial<Stage>): void {
    settings = {
      ...settings,
      stages: settings.stages.map((step) =>
        step.id === id ? { ...(step as Stage), ...change } : step,
      ),
    }
  }

  async function testSpecRunning(): Promise<Task> {
    const task = await report(await implementing('spec'), '1')
    expect(task).toMatchObject({ stage: 'test-spec', state: 'running' })
    return task
  }

  // T-0136: Test spec runs; the settings then give Spec its own agent and take Test spec's away.
  it('keeps the stage of a running agent when the settings change', async () => {
    const task = await testSpecRunning()
    const started = tasks
      .listEvents(task.id)
      .filter((entry) => entry.event.type === 'stage.started')
    expect(started.at(-1)?.event.payload).toMatchObject({ stage: 'test-spec', terminal: '7' })
    changeStage('spec', { output: 'spec.md', ownShell: true, agent: 'codex' })
    changeStage('test-spec', { agent: undefined, endShell: true })

    expect(runner.laneFor(task, '7')).toEqual({ stage: 'test-spec', state: 'running' })
    expect(runner.laneOfTerminal(task, '7')).toMatchObject({ stage: 'test-spec', state: 'running' })
    expect(runner.laneFor(task, '5')).toBeNull()
    writeWorking(task, 'test-spec.md', '# Cases\nx\n')

    const next = await report(task, '7')

    expect(next).toMatchObject({ stage: 'implement', state: 'running' })
    // Its endShell closes the shell it ran in.
    expect(terminals.list(task.id)).toEqual(['1'])
  })

  it('sends failed checks to the agent of the pinned shell after the settings change', async () => {
    const task = await testSpecRunning()
    const shell = ptys.at(-1)
    changeStage('test-spec', { agent: undefined })

    await report(task, '7')

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'test-spec', state: 'running' })
    await expectSubmitted(shell, 'The checks of this stage failed')
  })

  it('starts the agent of the pinned shell again when the daemon starts', async () => {
    const task = await testSpecRunning()
    changeStage('test-spec', { agent: undefined })
    terminals.close(task.id)

    runner.resumeAgents()
    await runner.settle()

    expect(terminals.list(task.id)).toContain('7')
    expect(terminals.list(task.id)).not.toContain('5')
  })

  it('runs a stage that lost its own agent in Shell 5 at its next start', async () => {
    const task = await testSpecRunning()
    writeWorking(task, 'test-spec.md', '# Cases\nx\n')
    await report(task, '7')
    changeStage('test-spec', { agent: undefined })

    await runner.act(task.id, 'reopen', { to: 'test-spec', reason: 'More cases' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'test-spec', state: 'running' })
    expect(launchScript(task.id, '5')).toMatch(/^claude /)
    expect(runner.laneFor(tasks.getTask(task.id), '5')).toEqual({
      stage: 'test-spec',
      state: 'running',
    })
    expect(runner.laneFor(tasks.getTask(task.id), '7')).toBeNull()
  })

  it('starts a stage that gained its own agent in a shell of its own at its next start', async () => {
    const task = await testSpecRunning()
    const copilotShell = ptys.at(-1)
    changeStage('spec', { output: 'spec.md', ownShell: true, agent: 'codex' })

    await runner.act(task.id, 'reopen', { to: 'spec', reason: 'Name the edge cases' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'running' })
    expect(launchScript(task.id, '6')).toMatch(/^codex /)
    expect(runner.laneFor(tasks.getTask(task.id), '6')).toEqual({ stage: 'spec', state: 'running' })
    expect(copilotShell?.written.join('')).not.toContain('Name the edge cases')
    expect(ptys.some((pty) => pty.killed)).toBe(false)
  })

  it('refuses a report from the pinned shell of a stage that does not run now', async () => {
    const task = await testSpecRunning()
    writeWorking(task, 'test-spec.md', '# Cases\nx\n')
    await report(task, '7')
    changeStage('test-spec', { agent: undefined })

    await expect(runner.act(task.id, 'report', { terminal: '7' })).rejects.toThrow(
      /test-spec stage of Terminal 7 does not run now/,
    )
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

  const failure: CiResult = {
    passed: false,
    reasons: ['CI E2E failure: u'],
    logs: ['CI E2E failure: u\n\x1b[31mError: 2 tests failed\x1b[0m'],
    passes: [],
    final: false,
  }

  // The paste that holds `text`, and the Enter that went apart after it.
  async function expectPromptSubmitted(text: string): Promise<string> {
    let paste = ''
    await vi.waitFor(() => {
      const written = ptys[0]?.written ?? []
      const at = written.findIndex((data) => data.startsWith('\x1b[200~') && data.includes(text))
      expect(at).toBeGreaterThanOrEqual(0)
      expect(written[at + 1]).toBe('\r')
      paste = written[at] ?? ''
    })
    return paste
  }

  it('sends the stage back to its agent with the log, as a review does', async () => {
    const task = await reported(true)

    finishCi(failure)
    await runner.settle()

    expect(lastOf(task.id, 'stage.check-failed')?.payload).toEqual({
      stage: 'implement',
      reasons: ['CI E2E failure: u'],
      final: false,
    })
    const reopened = lastOf(task.id, 'stage.reopened')?.payload
    expect(reopened).toMatchObject({
      to: 'implement',
      from: 'implement',
      byAgent: true,
      ci: true,
    })
    expect(reopened?.reason).toMatch(/^CI failed on Implement:\n- CI E2E failure: u\n/)
    expect(reopened?.reason).toContain('gh run view')
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    const starts = tasks.listEvents(task.id).filter(({ event }) => event.type === 'stage.started')
    expect(starts).toHaveLength(2)

    const paste = await expectPromptSubmitted('What to change: CI failed on Implement:')
    expect(paste).toContain('Error: 2 tests failed')
    expect(paste).not.toContain('\x1b[31m')
    expect(ptys[0]?.written.join('')).not.toContain('The checks of this stage failed')
    const prompts = (ptys[0]?.written ?? []).filter((data) => data.includes('What to change'))
    expect(prompts).toHaveLength(1)
  })

  it('names each failed run, keeps a run without a log, and uses the name of the stage', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [{ id: 'implement', name: 'Build it', kind: 'agent', gate: 'manual', ci: true }],
    }
    const task = await reported(true)

    finishCi({
      ...failure,
      reasons: ['CI E2E failure: u1', 'CI Lint failure: u2'],
      logs: ['CI E2E failure: u1\n\x1b[31mError: 2 tests failed\x1b[0m\r\n', 'CI Lint failure: u2'],
    })
    await runner.settle()

    expect(lastOf(task.id, 'stage.reopened')?.payload.reason).toBe(
      [
        'CI failed on Build it:',
        '- CI E2E failure: u1',
        '- CI Lint failure: u2',
        '',
        'CI E2E failure: u1\nError: 2 tests failed',
        'CI Lint failure: u2',
        'Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url.',
      ].join('\n'),
    )
    await expectPromptSubmitted('What to change: CI failed on Build it:')
  })

  it('starts the agent again with the failure when it ended meanwhile', async () => {
    const task = await reported(true)
    clock += LAUNCH_GRACE_MS + 1

    finishCi(failure)
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    const typed = ptys[0]?.written.join('') ?? ''
    expect(typed.split(`agent-launch-${task.id}.sh`)).toHaveLength(3)
    expect(launchScript(task.id)).toContain('What to change: CI failed on Implement:')
    expect(typed).not.toContain('What to change')
  })

  it('does not count send backs by a person toward the most times', async () => {
    const task = await implementing()
    tasks.setCi(task.id, 'implement', true)
    for (let round = 0; round < MAX_AGENT_SEND_BACKS; round += 1) {
      await runner.act(task.id, 'reopen', { to: 'implement', reason: 'Name it better' })
      await runner.settle()
    }
    await runner.act(task.id, 'report', { terminal: '1' })

    finishCi(failure)
    await runner.settle()

    expect(lastOf(task.id, 'stage.reopened')?.payload).toMatchObject({ ci: true })
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
  })

  it('keeps one more try for failed local checks, then sends a CI failure back', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [{ id: 'implement', kind: 'agent', gate: 'manual', ci: true, check: 'make test' }],
    }
    let code = 1
    runCommand = async () => ({ code, output: 'lint failed' })
    const task = await reported(true)

    expect(ciCalls).toEqual([])
    expect(tasks.getTask(task.id).state).toBe('running')
    await expectSubmitted(ptys[0], 'The checks of this stage failed')
    expect(lastOf(task.id, 'stage.reopened')).toBeUndefined()

    code = 0
    await runner.act(task.id, 'report', { terminal: '1' })
    expect(ciCalls).toHaveLength(1)
    finishCi(failure)
    await runner.settle()

    expect(lastOf(task.id, 'stage.reopened')?.payload).toMatchObject({ ci: true })
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
  })

  it('waits for you after CI sent the stage back the most times', async () => {
    const task = await reported(true)
    for (let round = 0; round < MAX_AGENT_SEND_BACKS; round += 1) {
      finishCi(failure)
      await runner.settle()
      expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
      await runner.act(task.id, 'report', { terminal: '1' })
    }

    finishCi(failure)
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'needs-input' })
    expect(lastOf(task.id, 'stage.check-failed')?.payload).toEqual({
      stage: 'implement',
      reasons: ['CI E2E failure: u'],
      final: true,
    })
    const sendBacks = tasks
      .listEvents(task.id)
      .filter(({ event }) => event.type === 'stage.reopened')
    expect(sendBacks).toHaveLength(MAX_AGENT_SEND_BACKS)
    // A paste waits for the Enter of the one before it.
    await vi.waitFor(() => {
      const prompts = (ptys[0]?.written ?? []).filter((data) => data.includes('What to change'))
      expect(prompts).toHaveLength(MAX_AGENT_SEND_BACKS)
    })
  })

  it.each([
    ['no run started', 'No CI run started for natamux-ci/x.'],
    ['CI took too long', 'CI did not finish within 30 minutes: u'],
  ])('waits for you at once when %s', async (_, reason) => {
    const task = await reported(true)

    finishCi({ passed: false, reasons: [reason], passes: [], final: true })
    await runner.settle()

    expect(lastOf(task.id, 'stage.check-failed')?.payload).toMatchObject({ final: true })
    expect(tasks.getTask(task.id).state).toBe('needs-input')
    expect(lastOf(task.id, 'stage.reopened')).toBeUndefined()
    const starts = tasks.listEvents(task.id).filter(({ event }) => event.type === 'stage.started')
    expect(starts).toHaveLength(1)
    expect(ptys[0]?.written.join('')).not.toContain(reason)
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

  // An empty worktree would run git in the daemon's own folder and push that checkout.
  it.each([
    ['workspace', true, false],
    ['branch', false, true],
    ['workspace and branch', true, true],
  ])('fails CI after a restart with no %s', async (_, noWorkspace, noBranch) => {
    const task = await reported(true)
    expect(ciCalls).toHaveLength(1)
    const failures = () =>
      tasks.listEvents(task.id).filter(({ event }) => event.type === 'stage.check-failed')

    runner.stop()
    tasks.recordWorkspace(task.id, {
      workspace: noWorkspace ? null : task.workspace,
      branch: noBranch ? null : task.branch,
    })
    runner = makeRunner(agents)
    runner.start()
    await runner.settle()

    expect(ciCalls).toHaveLength(1)
    expect(lastOf(task.id, 'stage.check-failed')?.payload).toEqual({
      stage: 'implement',
      reasons: ['The task has no workspace or branch for the CI check.'],
      final: true,
    })
    expect(tasks.getTask(task.id).state).toBe('needs-input')
    expect(lastOf(task.id, 'stage.reopened')).toBeUndefined()
    expect(failures()).toHaveLength(1)

    runner.stop()
    runner = makeRunner(agents)
    runner.start()
    await runner.settle()
    expect(ciCalls).toHaveLength(1)
    expect(failures()).toHaveLength(1)
  })

  it('does not wait for CI when the task did not ask for it', async () => {
    const task = await reported(false)

    expect(ciCalls).toEqual([])
    expect(tasks.getTask(task.id).state).toBe('review')
  })
})

describe('a CI failure on a testing stage', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'testing', kind: 'testing', gate: 'auto', ci: true },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  })

  const failure: CiResult = {
    passed: false,
    reasons: ['CI E2E failure: u'],
    logs: ['CI E2E failure: u\nError: 2 tests failed'],
    passes: [],
    final: false,
  }

  const lastOf = (id: string, type: EventType) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === type)
      .at(-1)?.event

  // Implement is done, the agent of Automated tests reports, and CI ends with `result`.
  async function failOnCi(taskId: string, result = failure): Promise<void> {
    await runner.act(taskId, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(taskId).stage).toBe('testing')
    await runner.act(taskId, 'report', { terminal: '1' })
    finishCi(result)
    await runner.settle()
  }

  async function withCi(): Promise<Task> {
    const task = await implementing()
    tasks.setCi(task.id, 'testing', true)
    return task
  }

  it('sends the task back to Implement with the failed runs, not their logs', async () => {
    const task = await withCi()

    await failOnCi(task.id)

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(lastOf(task.id, 'stage.check-failed')?.payload).toEqual({
      stage: 'testing',
      reasons: ['CI E2E failure: u'],
      final: false,
    })
    const reopened = lastOf(task.id, 'stage.reopened')?.payload
    expect(reopened).toMatchObject({ to: 'implement', from: 'testing', byAgent: true, ci: true })
    expect(reopened?.reason).toContain('CI E2E failure: u')
    expect(reopened?.reason).toContain('gh run view')
    expect(reopened?.reason).not.toContain('Error: 2 tests failed')
    await vi.waitFor(() => expect(ptys[0]?.written.join('')).toContain('gh run view'))
    expect(ptys[0]?.written.join('')).not.toContain('The checks of this stage failed')
  })

  it('counts the send backs of the testing agent toward the most times', async () => {
    const task = await withCi()
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    await runner.act(task.id, 'reopen', { terminal: '1', reason: 'Add a test for dates' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('implement')

    await failOnCi(task.id)
    expect(tasks.getTask(task.id).stage).toBe('implement')

    await failOnCi(task.id)
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'needs-input' })
  })

  it('waits for you after it sent the task back the most times', async () => {
    const task = await withCi()
    for (let round = 0; round < MAX_AGENT_SEND_BACKS; round += 1) {
      await failOnCi(task.id)
      expect(tasks.getTask(task.id).stage).toBe('implement')
    }

    await failOnCi(task.id)

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'needs-input' })
    expect(lastOf(task.id, 'stage.check-failed')?.payload).toEqual({
      stage: 'testing',
      reasons: ['CI E2E failure: u'],
      final: true,
    })
  })

  it('waits for you when no agent can fix it', async () => {
    const task = await withCi()

    await failOnCi(task.id, { ...failure, final: true })

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'needs-input' })
    expect(lastOf(task.id, 'stage.reopened')).toBeUndefined()
  })

  it('gives the failure to its own agent when no stage before it changes code', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [{ id: 'testing', kind: 'testing', gate: 'auto', ci: true }],
    }
    const task = await implementing('testing')
    tasks.setCi(task.id, 'testing', true)

    await runner.act(task.id, 'report', { terminal: '1' })
    finishCi(failure)
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'running' })
    await expectSubmitted(ptys[0], 'The checks of this stage failed')
    expect(ptys[0]?.written.join('')).toContain('Error: 2 tests failed')
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

  it('starts the agent and the words of a stage in a shell of its own, and keeps the task agent', async () => {
    withOwnAgent()
    const task = await atBuild()

    expect(launchScript(task.id)).toMatch(/^claude /)
    expect(launchScript(task.id, '9')).toMatch(/^codex /)
    expect(launchScript(task.id, '9')).toContain('--model gpt-5')
    expect(runner.laneFor(task, '9')).toEqual({ stage: 'test-spec', state: 'running' })
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

describe('a stage that ends its shell', () => {
  // Implement runs in Shell 1 with the task's agent; Write docs runs in Shell 7 with its own.
  function withStages(docs: Partial<Stage>, after: Stage[] = [], implement: Partial<Stage> = {}) {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto', ...implement },
        { id: 'write-docs', kind: 'agent', gate: 'auto', agent: 'codex', ...docs },
        ...after,
        { id: 'finish', kind: 'agent', gate: 'auto' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  }

  async function writingDocs(): Promise<Task> {
    const task = await implementing()
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'running' })
    return task
  }

  const launchesOf = (id: string, agent: string) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === 'agent.launched')
      .filter((entry) => entry.event.payload.agent === agent)

  it('closes its shell when the stage passes, and Shell 1 goes on with the next stage', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'finish', state: 'running' })
    expect(ptys).toHaveLength(2)
    expect(ptys[0]?.killed).toBe(false)
    expect(ptys[1]?.killed).toBe(true)
    expect(terminals.list(task.id)).toEqual(['1'])
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)
    await expectSubmitted(ptys[0], 'Task: Export')
  })

  it('drops the report of the closed agent, so a next agent in its shell reports its turn end', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'finish', state: 'running' })
    expect(runner.takeReport(task.id, '7')).toBe(false)
  })

  it('starts a new agent in a new shell with the send-back prompt, and closes it again', async () => {
    withStages({ endShell: true }, [{ id: 'qa', kind: 'manual', gate: 'manual' }])
    const task = await writingDocs()
    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'qa', state: 'review' })
    expect(terminals.list(task.id)).toEqual(['1'])

    await runner.act(task.id, 'reopen', { to: 'write-docs', reason: 'Fix the dates' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'running' })
    expect(ptys).toHaveLength(3)
    expect(ptys[2]?.killed).toBe(false)
    expect(launchesOf(task.id, 'codex')).toHaveLength(2)
    expect(launchScript(task.id, '7')).toContain('What to change: Fix the dates')

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('qa')
    expect(ptys[2]?.killed).toBe(true)
    expect(ptys[0]?.killed).toBe(false)
  })

  it('keeps the shell of a stage without it, as before', async () => {
    withStages({})
    const task = await writingDocs()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'finish', state: 'running' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(launchesOf(task.id, 'codex')).toHaveLength(1)
  })

  it('closes its shell when its agent sends the task back, and starts a new agent next round', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'reopen', { terminal: '7', to: 'implement', reason: 'Fix the dates' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(ptys[1]?.killed).toBe(true)
    expect(ptys[0]?.killed).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1'])
    // The killed agent never ends its turn, so nothing waits for it.
    expect(runner.takeReport(task.id, '7')).toBe(false)
    await expectWritten(ptys[0], 'What to change: Fix the dates')

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'running' })
    expect(ptys).toHaveLength(3)
    expect(ptys[2]?.killed).toBe(false)
    expect(launchesOf(task.id, 'codex')).toHaveLength(2)
    expect(launchScript(task.id, '7')).toMatch(/^codex /)
  })

  it('closes its shell when a person sends the task back', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'reopen', { to: 'implement' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(ptys[1]?.killed).toBe(true)
    expect(terminals.list(task.id)).toEqual(['1'])
  })

  it('keeps its shell when its agent sends the task back the most times', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()
    for (let round = 0; round < MAX_AGENT_SEND_BACKS; round += 1) {
      await runner.act(task.id, 'reopen', { terminal: '7', to: 'implement' })
      await runner.settle()
      await runner.act(task.id, 'report', { terminal: '1' })
      await runner.settle()
    }
    const shell = ptys.at(-1)

    await expect(runner.act(task.id, 'reopen', { terminal: '7', to: 'implement' })).rejects.toThrow(
      /waits for the user/,
    )

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'needs-input' })
    expect(shell?.killed).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1', '7'])
  })

  it('keeps its shell when the task goes back to the same stage', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'reopen', { to: 'write-docs', reason: 'Fix the dates' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'running' })
    expect(ptys[1]?.killed).toBe(false)
    expect(launchesOf(task.id, 'codex')).toHaveLength(1)
  })

  it('keeps the shell of a stage without it on a send back, and gives the agent there the prompt', async () => {
    withStages({})
    const task = await writingDocs()
    const shell = ptys[1]

    await runner.act(task.id, 'reopen', { terminal: '7', to: 'implement' })
    await runner.settle()
    expect(shell?.killed).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1', '7'])

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'running' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(launchesOf(task.id, 'codex')).toHaveLength(1)
    await expectSubmitted(shell, 'Task: Export')
  })

  it('never closes Shell 1 on a send back', async () => {
    withStages({}, [{ id: 'polish', kind: 'agent', gate: 'auto', endShell: true }])
    const task = await writingDocs()
    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('polish')

    await runner.act(task.id, 'reopen', { terminal: '1', to: 'implement' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(ptys[0]?.killed).toBe(false)
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)
  })

  it('keeps the shell while the stage waits for approval, and closes it on approval', async () => {
    withStages({ gate: 'manual', endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'write-docs', state: 'review' })
    expect(ptys[1]?.killed).toBe(false)

    await runner.act(task.id, 'approve')
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'finish', state: 'running' })
    expect(ptys[1]?.killed).toBe(true)
    expect(ptys[0]?.killed).toBe(false)
  })

  it('closes the shell of a skipped stage', async () => {
    withStages({ endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'skip')
    await runner.settle()

    expect(tasks.getTask(task.id).stage).toBe('finish')
    expect(ptys[1]?.killed).toBe(true)
    expect(ptys[0]?.killed).toBe(false)
  })

  it('keeps the shell of the stage before when a stage marked to skip is skipped', async () => {
    // Both run in Shell 5, so a planned skip of the second must not end the agent of the first.
    const report = (id: string, extra: Partial<Stage> = {}): Stage => ({
      id,
      kind: 'agent',
      gate: 'auto',
      output: `${id}.md`,
      ownShell: true,
      ...extra,
    })
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        report('check-docs'),
        report('check-names', { endShell: true }),
        { id: 'finish', kind: 'agent', gate: 'auto' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
    const task = await implementing()
    tasks.setSkip(task.id, 'check-names', true)
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('check-docs')
    expect(terminals.list(task.id)).toEqual(['1', '5'])
    writeWorking(task, 'check-docs.md', '# Findings\nNone\n')

    await runner.act(task.id, 'report', { terminal: '5' })
    await runner.settle()

    expect(tasks.getTask(task.id).stage).toBe('finish')
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1', '5'])
  })

  it('never closes Shell 1, so its agent goes on with the next stage', async () => {
    withStages({}, [], { endShell: true })
    const task = await writingDocs()

    await runner.act(task.id, 'report', { terminal: '7' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'finish', state: 'running' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(launchesOf(task.id, 'claude-code')).toHaveLength(1)
    await expectSubmitted(ptys[0], 'Task: Export')
  })

  it('never closes Shell 1 when its stage is skipped', async () => {
    withStages({}, [], { endShell: true })
    const task = await implementing()

    await runner.act(task.id, 'skip')
    await runner.settle()

    expect(tasks.getTask(task.id).stage).toBe('write-docs')
    expect(ptys[0]?.killed).toBe(false)
  })

  // The build group: Implement keeps Shell 1, Test spec runs in Shell 2.
  async function building(endShell: string): Promise<Task> {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        'parallel' in step
          ? {
              ...step,
              parallel: step.parallel.map((stage) =>
                stage.id === endShell ? { ...stage, endShell: true } : stage,
              ),
            }
          : step,
      ),
    }
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    await withWorkspace(task.id)
    expect(ptys).toHaveLength(2)
    return tasks.getTask(task.id)
  }

  it('closes only the shell of the lane that is done in a group', async () => {
    const task = await building('test-spec')

    // Test spec waits for approval after a report; a skip makes its lane done at once.
    await runner.act(task.id, 'skip', { stage: 'test-spec' })
    await runner.settle()

    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    expect(ptys.filter((pty) => pty.killed)).toHaveLength(1)
    expect(terminals.list(task.id)).toEqual(['1'])
  })

  it('keeps Shell 1 when the lane there is done in a group', async () => {
    const task = await building('implement')

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'running', implement: 'done' })
    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1', '2'])
  })

  it('closes the shell of a command when the command passes', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'testing', kind: 'testing', gate: 'auto', command: 'make test', endShell: true },
        { id: 'finish', kind: 'agent', gate: 'auto' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
    const task = await implementing()
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('testing')
    expect(terminals.list(task.id)).toEqual(['1', '2'])

    await runner.act(task.id, 'pass', { stage: 'testing' })
    await runner.settle()

    expect(tasks.getTask(task.id).stage).toBe('finish')
    expect(ptys[1]?.killed).toBe(true)
    expect(terminals.list(task.id)).toEqual(['1'])
  })

  it('closes the shell of a command when the task is sent back from its stage', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'testing', kind: 'testing', gate: 'auto', command: 'make test', endShell: true },
        { id: 'finish', kind: 'agent', gate: 'auto' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
    const task = await implementing()
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(terminals.list(task.id)).toEqual(['1', '2'])

    await runner.act(task.id, 'reopen', { to: 'implement', reason: 'Fix the dates' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    expect(ptys[1]?.killed).toBe(true)
    expect(ptys[0]?.killed).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1'])
  })

  const lastEvent = (id: string, type: EventType) =>
    tasks
      .listEvents(id)
      .filter((entry) => entry.event.type === type)
      .at(-1)?.event

  // A testing stage of your own with its own agent in Shell 7 fails on CI, so the task goes back
  // to Implement.
  async function failedOnCi(endShell: boolean): Promise<Task> {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'smoke', kind: 'testing', gate: 'auto', agent: 'codex', ci: true, endShell },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
    const task = await implementing()
    tasks.setCi(task.id, 'smoke', true)
    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'smoke', state: 'running' })
    expect(terminals.list(task.id)).toEqual(['1', '7'])

    await runner.act(task.id, 'report', { terminal: '7' })
    finishCi({ passed: false, reasons: ['CI E2E failure: u'], passes: [], final: false })
    await runner.settle()
    expect(lastEvent(task.id, 'stage.reopened')?.payload).toMatchObject({ from: 'smoke', ci: true })
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'implement', state: 'running' })
    return task
  }

  it('closes its shell when CI sends the task back from it', async () => {
    const task = await failedOnCi(true)

    expect(ptys[1]?.killed).toBe(true)
    expect(ptys[0]?.killed).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1'])
  })

  it('keeps the shell of a stage without it when CI sends the task back', async () => {
    const task = await failedOnCi(false)

    expect(ptys.some((pty) => pty.killed)).toBe(false)
    expect(terminals.list(task.id)).toEqual(['1', '7'])
  })

  it('leaves the last stage to the finish of the task, which keeps the screens', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [{ id: 'implement', kind: 'agent', gate: 'auto', endShell: true }],
    }
    const task = await implementing()
    ptys[0]?.emit('last words of the agent\r\n')
    await terminals.settled(task.id, '1')

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('done')
    await vi.waitFor(() => expect(ptys[0]?.killed).toBe(true))
    const [shell] = readEndedShells(dataDir, task.id)
    expect(shell).toMatchObject({ name: '1', stage: 'implement' })
    expect(shell?.text).toContain('last words of the agent')
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

describe('a stage marked to skip', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'qa', kind: 'manual', gate: 'manual' },
        { id: 'docs', kind: 'agent', gate: 'auto' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  })

  const skips = (id: string) =>
    tasks
      .listEvents(id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.skipped')

  async function reportImplement(id: string): Promise<Task> {
    await runner.act(id, 'report', { terminal: '1' })
    await runner.settle()
    return tasks.getTask(id)
  }

  it('is skipped when the task reaches it, and the next stage runs', async () => {
    const task = await implementing()
    tasks.setSkip(task.id, 'qa', true)

    expect(await reportImplement(task.id)).toMatchObject({ stage: 'docs', state: 'running' })
    expect(skips(task.id)).toEqual([
      expect.objectContaining({ source: 'system', payload: { stage: 'qa', planned: true } }),
    ])
  })

  it('is skipped again after a send back, but runs when sent back to it', async () => {
    const task = await implementing()
    tasks.setSkip(task.id, 'qa', true)
    await reportImplement(task.id)

    await runner.act(task.id, 'reopen', { to: 'implement', reason: 'Fix the dates' })
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('implement')
    expect(await reportImplement(task.id)).toMatchObject({ stage: 'docs', state: 'running' })
    expect(skips(task.id)).toHaveLength(2)

    await runner.act(task.id, 'reopen', { to: 'qa' })
    await runner.settle()
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'qa', state: 'review' })
    expect(skips(task.id)).toHaveLength(2)
  })

  it.each(['pass', 'skip'] as const)('is skipped after %s of the stage before', async (action) => {
    const task = await implementing()
    tasks.setSkip(task.id, 'qa', true)

    await runner.act(task.id, action)
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'docs', state: 'running' })
  })

  it('is skipped after a review approval, in order, and a marked release ends the task', async () => {
    const task = await implementing()
    tasks.setSkip(task.id, 'docs', true)
    tasks.setSkip(task.id, 'release', true)
    expect(await reportImplement(task.id)).toMatchObject({ stage: 'qa', state: 'review' })

    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'review.approved',
      payload: { stage: 'qa' },
    })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('done')
    const moves = tasks
      .listEvents(task.id)
      .map((entry) => entry.event)
      .filter((event) => ['review.approved', 'stage.skipped'].includes(event.type))
      .map((event) => [event.type, event.payload.stage])
    expect(moves).toEqual([
      ['review.approved', 'qa'],
      ['stage.skipped', 'docs'],
      ['stage.skipped', 'release'],
    ])
    expect(() => tasks.setSkip(task.id, 'release', false)).toThrow()
  })

  it('runs when the mark is taken off', async () => {
    const task = await implementing()
    tasks.setSkip(task.id, 'qa', true)
    tasks.setSkip(task.id, 'qa', false)

    expect(await reportImplement(task.id)).toMatchObject({ stage: 'qa', state: 'review' })
    expect(skips(task.id)).toEqual([])
  })
})

describe('a stage of a group marked to skip', () => {
  async function atPlanReview(): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'plan' },
    })
    return withWorkspace(task.id)
  }

  it('is skipped, and the other stage of the group starts', async () => {
    const task = await atPlanReview()
    tasks.setSkip(task.id, 'test-spec', true)

    await runner.act(task.id, 'approve')
    await runner.settle()

    expect(tasks.getTask(task.id).lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    expect(() => launchScript(task.id, '2')).toThrow()
  })

  it('skips a group and the stage after it in one move, and answers with the next stage started', async () => {
    const task = await atPlanReview()
    for (const stage of ['test-spec', 'implement', 'testing']) {
      tasks.setSkip(task.id, stage, true)
    }

    const moved = await runner.act(task.id, 'approve')

    expect(moved).toMatchObject({ stage: 'agent-review', state: 'running' })
    const skipped = tasks
      .listEvents(task.id)
      .map((entry) => entry.event)
      .filter((event) => event.type === 'stage.skipped')
      .map((event) => event.payload.stage)
    expect(skipped).toEqual(['test-spec', 'implement', 'testing'])
  })

  it('skips the whole group when each of its stages is marked', async () => {
    const task = await atPlanReview()
    tasks.setSkip(task.id, 'test-spec', true)
    tasks.setSkip(task.id, 'implement', true)

    const moved = await runner.act(task.id, 'approve')

    expect(moved.stage).toBe('testing')
    await runner.settle()
    expect(tasks.getTask(task.id).stage).toBe('testing')
  })
})

describe('a branch', () => {
  const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual' }
  const afterPlan: StageBranch = {
    id: 'after-plan',
    paths: [
      {
        id: 'small',
        when: { type: 'output', file: 'context.md', contains: '[small]' },
        stages: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'docs', kind: 'agent', gate: 'auto' },
        ],
      },
      {
        id: 'pr-review',
        when: { type: 'command', run: 'test -f .no-review' },
        not: true,
        stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
      },
    ],
  }

  // Context, the steps given, the branch, then Release.
  function pipeline(...before: Step[]): void {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'context', kind: 'context', gate: 'manual', output: 'context.md' },
        ...before,
        afterPlan,
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }
  }

  beforeEach(() => {
    pipeline(plan)
    // There is no .no-review file, so Review PR holds whenever Small change does not. The context
    // has the title, so "[small]" in it picks Small change.
    runCommand = async (command) => ({ code: command === 'test -f .no-review' ? 1 : 0, output: '' })
  })

  const eventsOf = (id: string) => tasks.listEvents(id).map((entry) => entry.event)
  const lastOf = (id: string, type: EventType) =>
    eventsOf(id)
      .filter((event) => event.type === type)
      .at(-1)
  const startedStages = (id: string) =>
    eventsOf(id)
      .filter((event) => event.type === 'stage.started')
      .map((event) => event.payload.stage)

  async function atPlanReview(title: string): Promise<Task> {
    const task = await created({ title, agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'plan' },
    })
    return withWorkspace(task.id)
  }

  it('takes the path whose condition holds when the stage before is approved', async () => {
    const task = await atPlanReview('Export [small]')

    const moved = await runner.act(task.id, 'approve')
    await runner.settle()

    expect(moved).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
    expect(lastOf(task.id, 'stage.approved')?.payload).toEqual({
      stage: 'plan',
      path: 'small',
      because: 'context.md contains "[small]"',
    })
    expect(startedStages(task.id)).toContain('implement')
    expect(startedStages(task.id)).not.toContain('review')
  })

  it('takes a path whose command fails, with not, and runs the command in the worktree', async () => {
    const ran: string[] = []
    runCommand = async (command, cwd) => {
      ran.push(`${command} in ${cwd}`)
      return { code: 1, output: '' }
    }
    const task = await atPlanReview('Export')

    const moved = await runner.act(task.id, 'approve')
    await runner.settle()

    expect(moved).toMatchObject({ stage: 'review', state: 'review' })
    expect(moved.paths).toEqual({ 'after-plan': 'pr-review' })
    expect(lastOf(task.id, 'stage.approved')?.payload).toMatchObject({
      path: 'pr-review',
      because: 'test -f .no-review did not exit 0',
    })
    expect(ran).toEqual([`test -f .no-review in ${task.workspace}`])
    expect(startedStages(task.id)).not.toContain('implement')
  })

  it('goes on after the branch when no condition holds', async () => {
    runCommand = async () => ({ code: 0, output: '' })
    const task = await atPlanReview('Export')

    const moved = await runner.act(task.id, 'approve')

    expect(moved).toMatchObject({ stage: 'release', paths: { 'after-plan': '' } })
    expect(lastOf(task.id, 'stage.approved')?.payload).toEqual({
      stage: 'plan',
      because: 'No condition held',
    })
  })

  it('checks the conditions on approval, so an edit made while the stage waited counts', async () => {
    const task = await atPlanReview('Export')
    writeWorking(task, 'context.md', '# Export\n\nA [small] change after all.\n')

    const moved = await runner.act(task.id, 'approve')

    expect(moved.paths).toEqual({ 'after-plan': 'small' })
  })

  it.each([
    ['pass', 'stage.passed'],
    ['skip', 'stage.skipped'],
  ] as const)('enters the path after %s of the stage before', async (action, type) => {
    const task = await atPlanReview('Export [small]')

    const moved = await runner.act(task.id, action)

    expect(moved).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
    expect(lastOf(task.id, type)?.payload).toMatchObject({ stage: 'plan', path: 'small' })
  })

  it('enters the path when the checks pass a stage with an auto gate', async () => {
    pipeline({ ...plan, gate: 'auto' })
    const task = await implementing('plan')

    await runner.act(task.id, 'report', { terminal: '1' })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'review', state: 'review' })
    expect(lastOf(task.id, 'stage.checked')?.payload).toMatchObject({
      stage: 'plan',
      path: 'pr-review',
    })
  })

  it('enters the path once CI passes the stage before', async () => {
    pipeline({ ...plan, gate: 'auto', ci: true })
    const task = await implementing('plan')
    tasks.setCi(task.id, 'plan', true)

    await runner.act(task.id, 'report', { terminal: '1' })
    expect(tasks.getTask(task.id).stage).toBe('plan')
    finishCi({ passed: true, reasons: [], passes: ['CI passed: CI u'], final: false })
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'review', state: 'review' })
    expect(lastOf(task.id, 'stage.checked')?.payload).toMatchObject({ path: 'pr-review' })
  })

  it('enters the path when the context stage passes on its own', async () => {
    pipeline()

    const task = await created({ title: 'Export [small]' })

    expect(task).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
    expect(lastOf(task.id, 'stage.passed')?.payload).toMatchObject({
      stage: 'context',
      path: 'small',
    })
  })

  it('enters the path when the last stage of a group ends', async () => {
    pipeline({
      id: 'build',
      parallel: [
        { id: 'code', kind: 'agent', gate: 'auto' },
        { id: 'notes', kind: 'agent', gate: 'auto' },
      ],
    })
    const task = await created({ title: 'Export [small]' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'build' },
    })

    await runner.act(task.id, 'approve', { stage: 'code' })
    expect(tasks.getTask(task.id).paths).toEqual({})
    const moved = await runner.act(task.id, 'approve', { stage: 'notes' })

    expect(moved).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
    expect(lastOf(task.id, 'stage.approved')?.payload).toMatchObject({
      stage: 'notes',
      path: 'small',
    })
  })

  it('enters the path when a stage marked to skip before the branch is skipped', async () => {
    const task = tasks.createTask({ title: 'Export [small]', agent: 'claude-code' })
    tasks.setSkip(task.id, 'plan', true)
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({
      stage: 'implement',
      paths: { 'after-plan': 'small' },
    })
    expect(lastOf(task.id, 'stage.skipped')?.payload).toMatchObject({
      stage: 'plan',
      planned: true,
      path: 'small',
    })
  })

  it('goes from stage to stage in the path, then to the step after the branch', async () => {
    const task = await atPlanReview('Export [small]')
    await runner.act(task.id, 'approve')

    expect(await runner.act(task.id, 'pass')).toMatchObject({ stage: 'docs', state: 'running' })
    expect(await runner.act(task.id, 'pass')).toMatchObject({
      stage: 'release',
      paths: { 'after-plan': 'small' },
    })
    expect(startedStages(task.id)).not.toContain('review')
  })

  it('refuses a send back to a path not taken, and checks again after one to Plan', async () => {
    const task = await atPlanReview('Export [small]')
    await runner.act(task.id, 'approve')
    const count = eventsOf(task.id).length

    await expect(runner.act(task.id, 'reopen', { to: 'review' })).rejects.toThrow(
      'review is on a path this task did not take',
    )
    await expect(runner.act(task.id, 'reopen', { to: 'review' })).rejects.toThrow(StageInputError)
    await expect(runner.act(task.id, 'reopen', { to: 'docs' })).rejects.toThrow(StageInputError)
    expect(eventsOf(task.id)).toHaveLength(count)

    const back = await runner.act(task.id, 'reopen', { to: 'plan' })
    expect(back.stage).toBe('plan')
    expect(back.paths).toEqual({})
    await runner.settle()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'plan' },
    })
    await runner.act(task.id, 'approve')

    const approvals = eventsOf(task.id).filter((event) => event.type === 'stage.approved')
    expect(approvals.map((event) => event.payload.path)).toEqual(['small', 'small'])
    expect(tasks.getTask(task.id).stage).toBe('implement')
  })

  it('takes a send back to a stage of the taken path', async () => {
    const task = await atPlanReview('Export [small]')
    await runner.act(task.id, 'approve')
    await runner.act(task.id, 'pass')

    const back = await runner.act(task.id, 'reopen', { to: 'implement' })

    expect(back).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
  })

  it('keeps the path over a restart, and starts the stage of the path again', async () => {
    const task = await atPlanReview('Export [small]')
    await runner.act(task.id, 'approve')
    await runner.settle()
    terminals.close(task.id)

    const reloadedIndex = new TaskIndex(':memory:')
    const reloaded = new TaskService({
      dataDir,
      index: reloadedIndex,
      bus: new EventBus(),
      settingsOf: () => settings,
    })
    reloaded.init()
    reloaded.close()
    expect(reloaded.getTask(task.id)).toMatchObject({
      stage: 'implement',
      state: 'running',
      paths: { 'after-plan': 'small' },
    })
    reloadedIndex.close()

    const launches = ptys.length
    runner.resumeAgents()
    await runner.settle()
    expect(ptys.length).toBeGreaterThan(launches)
    expect(runner.laneFor(tasks.getTask(task.id), '1')).toEqual({
      stage: 'implement',
      state: 'running',
    })
  })

  it('drops the checks of a round that started again while the conditions ran', async () => {
    pipeline({ ...plan, gate: 'auto' })
    let asked: () => void = () => undefined
    const running = new Promise<void>((resolve) => {
      asked = resolve
    })
    let finish: (code: number) => void = () => undefined
    runCommand = (command) => {
      if (command !== 'test -f .no-review') {
        return Promise.resolve({ code: 0, output: '' })
      }
      asked()
      return new Promise((resolve) => {
        finish = (code) => resolve({ code, output: '' })
      })
    }
    const task = await implementing('plan')

    const report = runner.act(task.id, 'report', { terminal: '1' })
    await running
    await runner.act(task.id, 'reopen', { to: 'plan' })
    finish(1)
    await report
    await runner.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'plan', state: 'running', paths: {} })
    const checked = eventsOf(task.id).filter(
      (event) => event.type === 'stage.checked' && event.payload.stage === 'plan',
    )
    expect(checked).toEqual([])
  })

  it('takes no path or reason from the sender, and keeps the stage of the event', async () => {
    runCommand = async () => ({ code: 0, output: '' })
    const task = await atPlanReview('Export')

    const input = await runner.withPath(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { path: 'pr-review', because: 'I say so' },
    })

    expect(input.payload).toEqual({ stage: 'plan', because: 'No condition held' })
    const asked = await runner.withPath(task.id, {
      source: 'agent',
      type: 'agent.asked',
      payload: { stage: 'plan', path: 'small', because: 'I say so' },
    })
    expect(asked.payload).toEqual({ stage: 'plan' })
  })

  it('checks a path the task has, though its stages are off now', async () => {
    const notes: Stage = { id: 'notes', kind: 'agent', gate: 'auto' }
    // Small change first, Review PR, then an Otherwise path.
    const withSmall = (enabled: boolean) => {
      const branch: StageBranch = {
        ...afterPlan,
        paths: [
          ...afterPlan.paths.map((path) =>
            path.id === 'small'
              ? { ...path, stages: path.stages.map((stage) => ({ ...stage, enabled })) }
              : path,
          ),
          { id: 'other', stages: [notes] },
        ],
      }
      settings = {
        ...settings,
        stages: settings.stages.map((step) => (step.id === branch.id ? branch : step)),
      }
    }
    withSmall(true)
    const task = await atPlanReview('Export [small]')
    withSmall(false)

    const moved = await runner.act(task.id, 'approve')

    expect(moved).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
  })
})

describe('who a stage action came from', () => {
  const sourceOf = (taskId: string, type: string) =>
    tasks
      .listEvents(taskId)
      .filter(({ event }) => event.type === type)
      .at(-1)?.event.source

  it.each([
    { byAgent: true, source: 'agent' },
    { byAgent: false, source: 'human' },
    { byAgent: undefined, source: 'human' },
  ])('records reopen and fail with byAgent $byAgent as $source', async ({ byAgent, source }) => {
    const task = await implementing()
    await runner.act(task.id, 'reopen', { to: 'implement', byAgent })
    expect(sourceOf(task.id, 'stage.reopened')).toBe(source)
    await runner.settle()
    await runner.act(task.id, 'fail', { byAgent })
    expect(sourceOf(task.id, 'stage.failed')).toBe(source)
  })

  it('records a report with byAgent and no terminal as the agent', async () => {
    const task = await implementing()
    await runner.act(task.id, 'report', { byAgent: true })
    await runner.settle()
    expect(sourceOf(task.id, 'stage.reported')).toBe('agent')
  })

  it('records a report with no terminal and no byAgent as a person', async () => {
    const task = await implementing()
    await runner.act(task.id, 'report')
    await runner.settle()
    expect(sourceOf(task.id, 'stage.reported')).toBe('human')
  })

  // A start records the stage starting as the system; it must never look like a person's step.
  it('records nothing as a person for a start by an agent', async () => {
    const task = await created({ title: 'Export', agent: 'claude-code' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'draft', stage: 'spec' },
    })
    await withWorkspace(task.id)
    const before = tasks.listEvents(task.id).length
    await runner.act(task.id, 'start', { byAgent: true })
    await runner.settle()
    const after = tasks.listEvents(task.id).slice(before)
    expect(after.filter(({ event }) => event.source === 'human')).toEqual([])
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'running' })
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
    runner.onOutput(task.id, '1', Buffer.from(exited))
    await runner.act(task.id, 'reopen', { to: 'spec' })
    expect(pty.written.join('')).toContain('\x1b[200~')

    runner.onOutput(task.id, '1', Buffer.from('\x1b]0;natamux-ag'))
    runner.onOutput(task.id, '1', Buffer.from('ent-exited\x07PS C:\\work> '))
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

  describe('on a stage whose agent failed', () => {
    // An API error ended the turn of the spec agent, as StopFailure reports it.
    async function failedSpec(): Promise<Task> {
      const task = await withWorkspace(
        (await created({ title: 'Export tasks', agent: 'claude-code' })).id,
      )
      tasks.applyEvent(task.id, {
        source: 'agent',
        type: 'agent.failed',
        payload: { stage: 'spec', sessionId: 's-1' },
      })
      expect(tasks.getTask(task.id).state).toBe('failed')
      return tasks.getTask(task.id)
    }

    // The events Start agent logged after the failure: one resume per failed stage, by a person.
    // The record of a launch is left out: it only notes that an agent started again.
    function loggedAfterFailure(taskId: string) {
      const events = tasks.listEvents(taskId)
      const failed = events.map(({ event }) => event.type).lastIndexOf('agent.failed')
      return events
        .slice(failed + 1)
        .filter(({ event }) => event.source === 'human' && event.type !== 'agent.launched')
        .map(({ accepted, event }) => ({ accepted, type: event.type, stage: event.payload.stage }))
    }

    it('asks the agent that still runs to go on', async () => {
      const task = await failedSpec()
      const pty = ptys.at(-1) as FakePty
      pty.program = 'node'
      const written = pty.written.length

      const after = await runner.startByHand(task.id, 'claude-code')
      await runner.settle()

      expect(after).toMatchObject({ stage: 'spec', state: 'running' })
      expect(loggedAfterFailure(task.id)).toEqual([
        { accepted: true, type: 'agent.resumed', stage: 'spec' },
      ])
      expect(ptys).toHaveLength(1)
      const typed = pty.written.slice(written).join('')
      expect(typed.split(`\x1b[200~${GO_ON_PROMPT}`)).toHaveLength(2)
      expect(typed).not.toContain('agent-launch-')
    })

    // A quick double click sends it twice.
    it('asks the agent to go on once when a person starts it twice', async () => {
      const task = await failedSpec()
      const pty = ptys.at(-1) as FakePty
      pty.program = 'node'
      const written = pty.written.length

      await runner.startByHand(task.id, 'claude-code')
      await runner.startByHand(task.id, 'claude-code')
      await runner.settle()

      expect(loggedAfterFailure(task.id)).toEqual([
        { accepted: true, type: 'agent.resumed', stage: 'spec' },
      ])
      const typed = pty.written.slice(written).join('')
      expect(typed.split(`\x1b[200~${GO_ON_PROMPT}`)).toHaveLength(2)
      expect(ptys).toHaveLength(1)
    })

    it('starts the agent again on the session of the stage when none runs', async () => {
      const task = await failedSpec()
      terminals.close(task.id)

      const after = await runner.startByHand(task.id, 'claude-code')
      await runner.settle()

      expect(after).toMatchObject({ stage: 'spec', state: 'running' })
      expect(loggedAfterFailure(task.id)).toEqual([
        { accepted: true, type: 'agent.resumed', stage: 'spec' },
      ])
      expect(terminals.list(task.id)).toEqual(['1'])
      expect(launchScript(task.id)).toContain("--resume 's-1'")
    })

    // As Resume does: the stage goes on where it stopped, with its own agent.
    it('goes on with the agent of the stage, whichever agent the person picked', async () => {
      const task = await failedSpec()
      terminals.close(task.id)

      await runner.startByHand(task.id, 'codex')
      await runner.settle()

      expect(tasks.getTask(task.id).state).toBe('running')
      expect(launchScript(task.id)).toContain("--resume 's-1'")
    })

    it('starts only the failed stage of a group, in its own terminal', async () => {
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
      tasks.applyEvent(task.id, {
        source: 'agent',
        type: 'agent.failed',
        payload: { stage: 'implement', sessionId: 's-1' },
      })
      terminals.close(task.id)

      await runner.startByHand(task.id, 'claude-code')
      await runner.settle()

      expect(loggedAfterFailure(task.id)).toEqual([
        { accepted: true, type: 'agent.resumed', stage: 'implement' },
      ])
      expect(tasks.getTask(task.id).lanes).toEqual({
        'test-spec': 'needs-input',
        implement: 'running',
      })
      expect(terminals.list(task.id)).toEqual(['1'])
      expect(launchScript(task.id)).toContain("--resume 's-1'")
    })
  })

  it('ends the shells of a task that is cancelled', async () => {
    const { task, pty } = await atSpecWithShell('node')

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })
    await runner.settle()

    expect(pty.killed).toBe(true)
    expect(terminals.list(task.id)).toEqual([])
  })

  it('keeps the last screen of each shell of a task that is cancelled', async () => {
    const { task, pty } = await atSpecWithShell('node')
    // The end of the screen is kept; the prompt pasted into shell 1 may be longer than that.
    pty.emit('last line of shell 1\r\n')
    await terminals.settled(task.id, '1')
    terminals.run(task.id, 'ls', '2')
    await terminals.settled(task.id, '2')

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })
    await runner.settle()

    const shells = readEndedShells(dataDir, task.id)
    const at = '2026-10-09T09:00:00.000Z'
    // Shell 1 ran the spec stage last; no stage used shell 2.
    expect(shells.map(({ text: _, ...shell }) => shell)).toEqual([
      { name: '1', agent: null, stage: 'spec', session: null, endedAt: at },
      { name: '2', agent: null, stage: null, session: null, endedAt: at },
    ])
    expect(shells[0]?.text.endsWith('last line of shell 1')).toBe(true)
    expect(shells[1]?.text).toContain('ls')
  })

  it('keeps a shell where a program that is no agent runs, such as a dev server', async () => {
    const { task, pty } = await atSpecWithShell('node')
    pty.commandLines = ['node /work/node_modules/.bin/claude --settings x.json']
    terminals.run(task.id, 'pnpm dev', '2')
    const server = ptys.at(-1) as FakePty
    server.program = 'node'
    server.commandLines = ['node /work/node_modules/.bin/vite']

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })
    await runner.settle()

    expect(pty.killed).toBe(true)
    expect(server.killed).toBe(false)
    expect(terminals.list(task.id)).toEqual(['2'])
    expect(readEndedShells(dataDir, task.id).map((shell) => shell.agent)).toEqual([
      'claude-code',
      null,
    ])
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
    expect(launchScript(task.id, '9')).toMatch(/^codex /)
    terminals.close(task.id)

    await resume(task.id, { stage: 'implement' })

    expect(launchScript(task.id)).toContain("--resume 's-1'")
  })

  it('keeps the Claude Code session of a stage after an agent came back in another shell', async () => {
    const task = await waitingAfterRestart({ sessionId: 's-1' })
    tasks.recordAgentLaunch(task.id, 'copilot', 'copilot', undefined, '6')

    await resume(task.id)

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

  // The Enter of a prompt would answer the menu of an agent that asked.
  it.each(['claude-code', 'codex'])('types nothing when %s still runs and asked', async (agent) => {
    const task = await withWorkspace((await created({ title: 'Export tasks', agent })).id)
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.asked',
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

  it.each(['claude-code', 'codex'])('asks %s to go on when it ended its turn', async (agent) => {
    const task = await withWorkspace((await created({ title: 'Export tasks', agent })).id)
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    const written = pty.written.length

    await resume(task.id)
    await resume(task.id)

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(ptys).toHaveLength(1)
    const typed = pty.written.slice(written).join('')
    expect(typed.split(`\x1b[200~${GO_ON_PROMPT}`)).toHaveLength(2)
    expect(typed).not.toContain('agent-launch-')
  })

  // The daemon started the agent again with its session; it is still at the end of that turn.
  it('asks the agent to go on after a restart started it again', async () => {
    const task = await withWorkspace(
      (await created({ title: 'Export tasks', agent: 'claude-code' })).id,
    )
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    tasks.recordAgentLaunch(task.id, 'claude-code', 'claude')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    const written = pty.written.length

    await resume(task.id)

    expect(pty.written.slice(written).join('')).toContain(`\x1b[200~${GO_ON_PROMPT}`)
  })

  // Only what the agent did tells a turn end from a question; a CI result or an edit does not.
  it('asks the agent to go on after an event that is not from the agent', async () => {
    const task = await withWorkspace(
      (await created({ title: 'Export tasks', agent: 'claude-code' })).id,
    )
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    tasks.updateTask(task.id, { title: 'Export all tasks' })
    expect(tasks.listEvents(task.id).at(-1)?.event.type).toBe('task.edited')
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    const written = pty.written.length

    await resume(task.id)

    expect(pty.written.slice(written).join('')).toContain(`\x1b[200~${GO_ON_PROMPT}`)
  })

  it('types nothing for a prompt typed to an agent that runs', async () => {
    const task = await withWorkspace(
      (await created({ title: 'Export tasks', agent: 'claude-code' })).id,
    )
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    const pty = ptys.at(-1) as FakePty
    pty.program = 'node'
    const written = pty.written.length

    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.resumed',
      payload: { stage: 'spec', sessionId: 's-1' },
    })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('running')
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

  describe('when the daemon starts', () => {
    async function resumeAll(): Promise<void> {
      runner.resumeAgents()
      await runner.settle()
    }

    // The agent was at work on the spec, after a turn that reported its session.
    async function runningAfterRestart(sessionId: string, agent = 'claude-code'): Promise<Task> {
      const task = await waitingAfterRestart({ sessionId }, agent)
      tasks.applyEvent(task.id, {
        source: 'agent',
        type: 'agent.resumed',
        payload: { stage: 'spec', sessionId },
      })
      return tasks.getTask(task.id)
    }

    it('goes on with the session of a running stage, with no new prompt', async () => {
      const task = await runningAfterRestart('s-1')
      expect(task.state).toBe('running')

      await resumeAll()

      expect(tasks.getTask(task.id).state).toBe('running')
      expect(typedLaunch(task.id)).toBe(true)
      expect(launchScript(task.id)).toContain("--resume 's-1'")
      expect(launchScript(task.id)).not.toContain('Task: Export tasks')
    })

    it('goes on with the session of a stage that waits for you, and keeps it waiting', async () => {
      const task = await waitingAfterRestart({ sessionId: 's-1' })

      await resumeAll()

      expect(tasks.getTask(task.id).state).toBe('needs-input')
      expect(launchScript(task.id)).toContain("--resume 's-1'")
    })

    // Puts a stage in review, as when it waits for approval, with no shell left.
    async function toReview(taskId: string, stage = 'spec'): Promise<Task> {
      tasks.applyEvent(taskId, {
        source: 'human',
        type: 'state.overridden',
        payload: { to: 'review', stage },
      })
      await runner.settle()
      terminals.close(taskId)
      return tasks.getTask(taskId)
    }

    // The agent said the stage is done; the stage waits for approval.
    async function inReviewAfterRestart(sessionId: string, agent = 'claude-code'): Promise<Task> {
      return toReview((await waitingAfterRestart({ sessionId }, agent)).id)
    }

    it('goes on with the session of a stage in review, and keeps it in review', async () => {
      const task = await inReviewAfterRestart('s-1')
      expect(task.state).toBe('review')

      await resumeAll()

      expect(tasks.getTask(task.id).state).toBe('review')
      expect(typedLaunch(task.id)).toBe(true)
      expect(launchScript(task.id)).toContain("--resume 's-1'")
      expect(launchScript(task.id)).not.toContain('Task: Export tasks')
    })

    it('goes on with the last session of a stage in review', async () => {
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
      await toReview(task.id)

      await resumeAll()

      expect(launchScript(task.id)).toContain("--resume 's-2'")
      expect(launchScript(task.id)).not.toContain('s-1')
    })

    it('goes on with the newest session of the folder for a review with none', async () => {
      const task = await toReview((await waitingAfterRestart({})).id)

      await resumeAll()

      expect(launchScript(task.id)).toContain('--continue')
      expect(launchScript(task.id)).not.toContain('Task: Export tasks')
      expect(tasks.getTask(task.id).state).toBe('review')
    })

    it('leaves the agent of a stage in review alone on a second start', async () => {
      const task = await inReviewAfterRestart('s-1')
      await resumeAll()
      for (const pty of ptys) {
        pty.program = 'node'
      }
      const before = tasks.listEvents(task.id).length
      const written = ptys.map((pty) => pty.written.length)

      await resumeAll()

      expect(tasks.listEvents(task.id)).toHaveLength(before)
      expect(ptys.map((pty) => pty.written.length)).toEqual(written)
      expect(tasks.getTask(task.id).state).toBe('review')
    })

    it.each([
      ['in review', inReviewAfterRestart],
      ['running', runningAfterRestart],
    ])('starts Codex %s again with the stage prompt, even with a session', async (_name, make) => {
      const task = await make('s-1', 'codex')

      await resumeAll()

      expect(tasks.getTask(task.id).state).toBe(task.state)
      expect(launchScript(task.id)).toMatch(/^codex /)
      expect(launchScript(task.id)).toContain('Task: Export tasks')
      expect(launchScript(task.id)).not.toMatch(/--continue|--resume/)
    })

    it('goes on with the session of a lane in review and of a waiting lane', async () => {
      const task = await toReview((await buildAfterRestart()).id, 'implement')
      expect(task.lanes).toEqual({ implement: 'review', 'test-spec': 'needs-input' })

      await resumeAll()

      expect(launchScript(task.id)).toContain("--resume 's-1'")
      expect(launchScript(task.id)).not.toContain('s-2')
      expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
      expect(launchScript(task.id, '2')).not.toContain('s-1')
      expect(tasks.getTask(task.id)).toMatchObject({ state: task.state, lanes: task.lanes })
    })

    // Both lanes work in the same folder, so its newest session may be the other lane's.
    it('starts a lane of a group in review with no session with its prompt', async () => {
      const { id } = await created({ title: 'Export tasks', agent: 'claude-code' })
      tasks.applyEvent(id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to: 'ready', stage: 'build' },
      })
      await withWorkspace(id)
      const task = await toReview(id, 'implement')

      await resumeAll()

      expect(launchScript(task.id)).toContain('Task: Export tasks')
      expect(launchScript(task.id)).not.toMatch(/--continue|--resume/)
      expect(tasks.getTask(task.id).lanes.implement).toBe('review')
    })

    it.each(['blocked', 'failed'])('leaves alone a %s lane beside one in review', async (to) => {
      const task = await toReview((await buildAfterRestart()).id, 'implement')
      tasks.applyEvent(task.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to, stage: 'test-spec' },
      })
      await runner.settle()
      terminals.close(task.id)

      await resumeAll()

      expect(terminals.list(task.id)).toEqual(['1'])
      expect(launchScript(task.id)).toContain("--resume 's-1'")
    })

    it('goes on with the session of each stage of a group in review', async () => {
      const task = await buildAfterRestart()
      tasks.applyEvent(task.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to: 'review' },
      })
      expect(tasks.getTask(task.id).lanes).toEqual({ implement: 'review', 'test-spec': 'review' })

      await resumeAll()

      expect(launchScript(task.id)).toContain("--resume 's-1'")
      expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
      expect(tasks.getTask(task.id).lanes).toEqual({ implement: 'review', 'test-spec': 'review' })
    })

    it.each([
      ['a review by a person', 'review', withManualReview],
      ['the release', 'release', () => undefined],
      ['tests a command runs', 'testing', withTestCommand],
    ])('leaves alone a stage in review that no agent runs: %s', async (_name, stage, configure) => {
      configure()
      const other = await inReviewAfterRestart('s-1')
      const task = await created({ title: 'Export tasks', agent: 'claude-code' })
      tasks.applyEvent(task.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to: 'review', stage },
      })
      await withWorkspace(task.id)
      await runner.settle()
      terminals.close(task.id)
      const before = tasks.listEvents(task.id).length

      await resumeAll()

      expect(terminals.list(task.id)).toEqual([])
      expect(tasks.listEvents(task.id)).toHaveLength(before)
      expect(tasks.getTask(task.id)).toMatchObject({ stage, state: 'review' })
      expect(launchScript(other.id)).toContain("--resume 's-1'")
    })

    // Only a running stage takes the failed check. A stage in review already waits for you, so it
    // stays there with no agent, as a waiting one does.
    it.each([
      ['first', true],
      ['last', false],
    ])('keeps a stage in review when its agent cannot start, %s', async (_name, reviewFirst) => {
      const early = reviewFirst ? undefined : await runningAfterRestart('s-q')
      const review = await inReviewAfterRestart('s-p')
      const running = early ?? (await runningAfterRestart('s-q'))
      const run = terminals.run.bind(terminals)
      vi.spyOn(terminals, 'run').mockImplementation((taskId, ...rest) => {
        if (taskId === review.id) {
          throw new Error('No shell')
        }
        run(taskId, ...rest)
      })

      await resumeAll()

      expect(tasks.getTask(review.id).state).toBe('review')
      expect(terminals.list(review.id)).toEqual([])
      expect(tasks.listEvents(review.id).at(-1)).toMatchObject({
        accepted: false,
        event: { type: 'stage.check-failed', payload: { stage: 'spec', reasons: ['No shell'] } },
      })
      expect(tasks.getTask(running.id).state).toBe('running')
      expect(launchScript(running.id)).toContain("--resume 's-q'")
    })

    it('records the reason for each agent that cannot start', async () => {
      const review = await inReviewAfterRestart('s-p')
      const running = await runningAfterRestart('s-q')
      vi.spyOn(terminals, 'run').mockImplementation(() => {
        throw new Error('No shell')
      })

      await resumeAll()

      for (const task of [review, running]) {
        expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
          type: 'stage.check-failed',
          payload: { stage: 'spec', reasons: ['No shell'] },
        })
      }
      expect(tasks.getTask(review.id).state).toBe('review')
      expect(tasks.getTask(running.id).state).toBe('needs-input')
    })

    it('keeps the other lane of a group going when a lane in review cannot start', async () => {
      const task = await toReview((await buildAfterRestart()).id, 'implement')
      const run = terminals.run.bind(terminals)
      vi.spyOn(terminals, 'run').mockImplementation((taskId, line, terminal, size) => {
        if (terminal === undefined || terminal === '1') {
          throw new Error('No shell')
        }
        run(taskId, line, terminal, size)
      })

      await resumeAll()

      expect(terminals.list(task.id)).toEqual(['2'])
      expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
      expect(tasks.getTask(task.id).lanes).toEqual({
        implement: 'review',
        'test-spec': 'needs-input',
      })
    })

    it('goes on with the newest session of the folder before the stage reported one', async () => {
      const { id } = await created({ title: 'Export tasks', agent: 'claude-code' })
      const task = await withWorkspace(id)
      expect(task.state).toBe('running')
      terminals.close(task.id)

      await resumeAll()

      expect(launchScript(task.id)).toContain('--continue')
      expect(launchScript(task.id)).not.toContain('Task: Export tasks')
    })

    it('goes on with the session of each stage of a group, in its own terminal', async () => {
      const task = await buildAfterRestart()

      await resumeAll()

      expect(terminals.list(task.id).sort()).toEqual(['1', '2'])
      expect(launchScript(task.id)).toContain("--resume 's-1'")
      expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
      expect(tasks.getTask(task.id).lanes).toEqual({
        implement: 'needs-input',
        'test-spec': 'needs-input',
      })
    })

    // Both stages run in the same folder, so its newest session may be the other stage's.
    it('starts a stage of a group with no session with its prompt', async () => {
      const task = await created({ title: 'Export tasks', agent: 'claude-code' })
      tasks.applyEvent(task.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to: 'ready', stage: 'build' },
      })
      await withWorkspace(task.id)
      terminals.close(task.id)

      await resumeAll()

      for (const terminal of ['1', '2']) {
        expect(launchScript(task.id, terminal)).toContain('Task: Export tasks')
        expect(launchScript(task.id, terminal)).not.toMatch(/--continue|--resume/)
      }
    })

    it.each(otherAgents)('starts %s again with the stage prompt', async (agent) => {
      const task = await runningAfterRestart('s-1', agent)

      await resumeAll()

      expect(typedLaunch(task.id)).toBe(true)
      expect(launchScript(task.id)).toContain('Task: Export tasks')
      expect(launchScript(task.id)).not.toMatch(/--continue|--resume/)
    })

    it.each(['running', 'review'])('leaves alone the tasks no agent works on, %s', async (to) => {
      const noWorkspace = await created({ title: 'Later', agent: 'claude-code' })
      tasks.applyEvent(noWorkspace.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to, stage: 'spec' },
      })
      // A person runs their own agent in the shell; Natamux never started one.
      const byHand = await created({ title: 'By hand' })
      runner.leaveToPerson(byHand.id)
      await withWorkspace(byHand.id)
      tasks.applyEvent(byHand.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to, stage: 'spec' },
      })
      terminals.close(byHand.id)
      await runner.settle()
      const before = [noWorkspace, byHand].map((task) => tasks.listEvents(task.id).length)

      await resumeAll()

      for (const [at, task] of [noWorkspace, byHand].entries()) {
        expect(terminals.list(task.id)).toEqual([])
        expect(tasks.listEvents(task.id)).toHaveLength(before[at] ?? 0)
        expect(tasks.getTask(task.id).state).toBe(to)
      }
    })

    it('starts the other agents when one cannot start', async () => {
      const first = await runningAfterRestart('s-1')
      const second = await runningAfterRestart('s-2')
      const run = terminals.run.bind(terminals)
      vi.spyOn(terminals, 'run')
        .mockImplementationOnce(() => {
          throw new Error('No shell')
        })
        .mockImplementation(run)

      await resumeAll()

      // The failed one waits for you with the reason, whichever came first.
      const after = [first, second].map((task) => ({
        state: tasks.getTask(task.id).state,
        shells: terminals.list(task.id),
      }))
      expect(after).toContainEqual({ state: 'needs-input', shells: [] })
      expect(after).toContainEqual({ state: 'running', shells: ['1'] })
    })

    it('adds only the launch to the events, and keeps each state', async () => {
      const made = [
        await runningAfterRestart('s-1'),
        await waitingAfterRestart({ sessionId: 's-2' }),
        await inReviewAfterRestart('s-3'),
      ]
      const before = made.map((task) => tasks.listEvents(task.id).length)

      await resumeAll()

      for (const [at, task] of made.entries()) {
        const added = tasks.listEvents(task.id).slice(before[at])
        expect(added.map(({ event }) => event.type)).toEqual(['agent.launched'])
        expect(tasks.getTask(task.id).state).toBe(task.state)
      }
    })

    it('resumes a running and a waiting stage of the same group', async () => {
      const task = await buildAfterRestart()
      tasks.applyEvent(task.id, {
        source: 'agent',
        type: 'agent.resumed',
        payload: { stage: 'implement', sessionId: 's-1' },
      })

      await resumeAll()

      expect(tasks.getTask(task.id).lanes).toEqual({
        implement: 'running',
        'test-spec': 'needs-input',
      })
      expect(launchScript(task.id)).toContain("--resume 's-1'")
      expect(launchScript(task.id, '2')).toContain("--resume 's-2'")
    })

    it.each([
      ['done', 'done'],
      ['cancelled', 'cancelled'],
      ['failed', 'failed'],
      ['blocked', 'blocked'],
      ['a draft', 'draft'],
    ] as const)('leaves alone a task that is %s', async (_name, to) => {
      const task = await waitingAfterRestart({ sessionId: 's-1' })
      tasks.applyEvent(task.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to, stage: 'spec' },
      })
      await runner.settle()
      terminals.close(task.id)
      const before = tasks.listEvents(task.id).length

      await resumeAll()

      expect(terminals.list(task.id)).toEqual([])
      expect(tasks.listEvents(task.id)).toHaveLength(before)
    })

    // Natamux cannot start it, so it must not fail the stage either.
    it('leaves alone a task whose agent Natamux cannot start', async () => {
      const task = await runningAfterRestart('s-1', 'opencode')
      const before = tasks.listEvents(task.id).length

      await resumeAll()

      expect(tasks.getTask(task.id).state).toBe('running')
      expect(terminals.list(task.id)).toEqual([])
      expect(tasks.listEvents(task.id)).toHaveLength(before)
    })

    it('pastes the prompt of a new start of the stage into the resumed agent', async () => {
      const task = await waitingAfterRestart({ sessionId: 's-1' })
      await resumeAll()
      const pty = ptys.at(-1) as FakePty

      await runner.act(task.id, 'reopen', { to: 'spec' })
      await runner.settle()

      await expectSubmitted(pty, 'Task: Export tasks')
      expect(pty.written.join('').split(`agent-launch-${task.id}.sh`)).toHaveLength(2)
    })

    it('resumes again after a second restart', async () => {
      const task = await runningAfterRestart('s-1')
      await resumeAll()
      terminals.close(task.id)

      await resumeAll()

      expect(terminals.list(task.id)).toEqual(['1'])
      expect(launchScript(task.id)).toContain("--resume 's-1'")
    })

    describe('a stage whose command ran', () => {
      const scriptOf = (id: string) => join(dataDir, `stage-command-${id}.sh`)

      // The test command runs in the task's workspace.
      async function testingRunning(input: Record<string, unknown> = {}): Promise<Task> {
        withTestCommand()
        const task = await created({ title: 'Export tasks', agent: 'claude-code', ...input })
        tasks.applyEvent(task.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'ready', stage: 'testing' },
        })
        await withWorkspace(task.id)
        expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'running' })
        return tasks.getTask(task.id)
      }

      // The daemon stops, which ends every shell, and starts again.
      function restart(): void {
        runner.stop()
        for (const task of tasks.listTasks()) {
          terminals.close(task.id)
        }
        runner = makeRunner(agents)
        runner.start()
      }

      // The test command ran when the daemon stopped: its shell is gone, the stage still runs.
      async function testingAfterRestart(input: Record<string, unknown> = {}): Promise<Task> {
        const task = await testingRunning(input)
        restart()
        return tasks.getTask(task.id)
      }

      it('runs the command again in a new shell, with no agent', async () => {
        const task = await testingAfterRestart()
        const shells = ptys.length

        await resumeAll()

        const script = scriptOf(task.id)
        expect(tasks.listEvents(task.id).slice(-2)).toMatchObject([
          {
            accepted: true,
            event: { source: 'system', type: 'agent.stopped', payload: { stage: 'testing' } },
          },
          { accepted: true, event: { type: 'stage.started', payload: { stage: 'testing' } } },
        ])
        expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'running' })
        expect(ptys).toHaveLength(shells + 1)
        expect(terminals.list(task.id)).toEqual(['2'])
        expect(ptys.at(-1)?.written).toEqual([`sh '${script}'\r`])
        expect(readFileSync(script, 'utf8')).toContain('make test')
      })

      it.each([
        ['no agent', null],
        ['an agent Natamux cannot start', 'aider'],
      ])('runs the command again for a task with %s', async (_name, agent) => {
        const task = await testingAfterRestart({ agent })

        await resumeAll()

        const types = tasks.listEvents(task.id).map(({ event }) => event.type)
        expect(types.slice(-2)).toEqual(['agent.stopped', 'stage.started'])
        expect(tasks.getTask(task.id).state).toBe('running')
        expect(terminals.list(task.id)).toEqual(['2'])
        expect(ptys.at(-1)?.written).toEqual([`sh '${scriptOf(task.id)}'\r`])
      })

      it('runs the command again for each task, in its own shell', async () => {
        const first = await testingRunning()
        const second = await testingRunning({ agent: null })
        restart()
        const shells = ptys.length

        await resumeAll()

        for (const task of [first, second]) {
          const types = tasks.listEvents(task.id).map(({ event }) => event.type)
          expect(types.slice(-2)).toEqual(['agent.stopped', 'stage.started'])
          expect(tasks.getTask(task.id).state).toBe('running')
          expect(terminals.list(task.id)).toEqual(['2'])
          expect(readFileSync(scriptOf(task.id), 'utf8')).toContain('make test')
        }
        const lines = ptys.slice(shells).flatMap((pty) => pty.written)
        const expected = [first, second].map((task) => `sh '${scriptOf(task.id)}'\r`)
        expect(lines.sort()).toEqual(expected.sort())
      })

      it('runs the command again when another task is at a stage the settings lost', async () => {
        const lost = await created({ title: 'Old task', agent: 'claude-code' })
        const task = await testingAfterRestart()
        const others = tasks.listTasks().filter((each) => each.id !== lost.id)
        vi.spyOn(tasks, 'listTasks').mockReturnValue([{ ...lost, stage: 'gone' }, ...others])

        expect(() => runner.resumeAgents()).not.toThrow()
        await runner.settle()

        expect(tasks.getTask(task.id).state).toBe('running')
        expect(terminals.list(task.id)).toEqual(['2'])
        expect(ptys.at(-1)?.written).toEqual([`sh '${scriptOf(task.id)}'\r`])
      })

      it('leaves alone a stage that waits for CI', async () => {
        const task = await testingAfterRestart()
        tasks.recordEvent(task.id, 'stage.ci-started', { stage: 'testing', branch: 'ci' })
        const before = tasks.listEvents(task.id).length

        await resumeAll()

        expect(tasks.listEvents(task.id)).toHaveLength(before)
        expect(terminals.list(task.id)).toEqual([])
        expect(tasks.getTask(task.id).state).toBe('running')
      })

      it('runs the command again once its CI check has a result', async () => {
        const task = await testingAfterRestart()
        tasks.recordEvent(task.id, 'stage.ci-started', { stage: 'testing', branch: 'ci' })
        tasks.recordEvent(task.id, 'stage.check-failed', {
          stage: 'testing',
          reasons: ['CI failed'],
        })

        await resumeAll()

        const types = tasks.listEvents(task.id).map(({ event }) => event.type)
        expect(types.slice(-2)).toEqual(['agent.stopped', 'stage.started'])
        expect(terminals.list(task.id)).toEqual(['2'])
      })

      // The settings schema keeps a testing stage out of a group, but the runner reads lanes the
      // same way for every kind.
      it('runs the command of a lane again, and resumes the agent of the other lane', async () => {
        settings = {
          ...DEFAULT_PROJECT_SETTINGS,
          stages: [
            {
              id: 'build',
              name: 'Build',
              parallel: [
                { id: 'implement', kind: 'agent', gate: 'auto' },
                { id: 'testing', kind: 'testing', gate: 'auto', command: 'make test' },
              ],
            },
            { id: 'release', kind: 'release', gate: 'manual' },
          ],
        }
        const made = await created({ title: 'Export tasks', agent: 'claude-code' })
        await withWorkspace(made.id)
        const task = await runner.act(made.id, 'start')
        expect(task.lanes).toEqual({ implement: 'running', testing: 'running' })
        tasks.applyEvent(task.id, {
          source: 'agent',
          type: 'agent.waiting',
          payload: { stage: 'implement', sessionId: 's-1' },
        })
        restart()

        await resumeAll()

        const payloadsOf = (type: EventType) =>
          tasks
            .listEvents(task.id)
            .filter(({ event }) => event.type === type)
            .map(({ event }) => event.payload)
        expect(payloadsOf('agent.stopped')).toEqual([{ stage: 'testing' }])
        const starts = payloadsOf('stage.started').filter(({ stage }) => stage === 'testing')
        expect(starts).toHaveLength(2)
        expect(tasks.getTask(task.id).lanes).toEqual({ implement: 'running', testing: 'running' })
        expect(launchScript(task.id)).toContain("--resume 's-1'")
        expect([...terminals.list(task.id)].sort()).toEqual(['1', '2'])
        expect(ptys.at(-1)?.written).toEqual([`sh '${scriptOf(task.id)}'\r`])
      })

      const notRunning = ['ready', 'needs-input', 'review', 'failed']
      it.each(notRunning)('leaves alone a %s stage', async (to) => {
        const task = await testingAfterRestart()
        tasks.applyEvent(task.id, { source: 'human', type: 'state.overridden', payload: { to } })
        await runner.settle()
        const before = tasks.listEvents(task.id).length

        await resumeAll()

        expect(tasks.listEvents(task.id)).toHaveLength(before)
        expect(terminals.list(task.id)).toEqual([])
        expect(tasks.getTask(task.id).state).toBe(to)
      })

      it('runs the command once the task gets its workspace', async () => {
        withTestCommand()
        const task = await created({ title: 'Export tasks', agent: 'claude-code' })
        tasks.applyEvent(task.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'ready', stage: 'testing' },
        })
        await runner.settle()
        tasks.applyEvent(task.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'running' },
        })
        await runner.settle()
        runner.stop()
        runner = makeRunner(agents)
        runner.start()

        await resumeAll()

        expect(tasks.getTask(task.id)).toMatchObject({ stage: 'testing', state: 'ready' })
        expect(terminals.list(task.id)).toEqual([])

        await withWorkspace(task.id)

        expect(tasks.getTask(task.id).state).toBe('running')
        expect(terminals.list(task.id)).toEqual(['2'])
      })
    })

    describe('a stage that waits', () => {
      // The daemon stops, which ends every shell, and starts again.
      async function restart(): Promise<void> {
        runner.stop()
        for (const task of tasks.listTasks()) {
          terminals.close(task.id)
        }
        runner = makeRunner(agents)
        runner.start()
        await resumeAll()
      }

      it('starts once the tasks it depends on are done', async () => {
        const first = await created({ title: 'Schema' })
        const task = await withWorkspace(
          (await created({ title: 'Export tasks', agent: 'claude-code', dependsOn: [first.id] }))
            .id,
        )
        expect(task).toMatchObject({ stage: 'spec', state: 'ready' })
        await restart()

        tasks.applyEvent(first.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'done' },
        })
        await runner.settle()

        expect(tasks.getTask(task.id).state).toBe('running')
        expect(launchScript(task.id)).toContain('Task: Export tasks')
      })

      it('starts once the task gets its workspace', async () => {
        const task = await created({ title: 'Export tasks', agent: 'claude-code' })
        expect(task).toMatchObject({ stage: 'spec', state: 'ready' })
        await restart()

        await withWorkspace(task.id)

        expect(tasks.getTask(task.id).state).toBe('running')
        expect(launchScript(task.id)).toContain('Task: Export tasks')
      })

      it('starts no agent in a task left to a person', async () => {
        const task = await created({ title: 'By hand' })
        runner.leaveToPerson(task.id)
        await withWorkspace(task.id)
        await restart()

        tasks.applyEvent(task.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'ready', stage: 'spec' },
        })
        await runner.settle()

        expect(tasks.getTask(task.id).state).toBe('ready')
        expect(terminals.list(task.id)).toEqual([])
      })

      it("starts no stage's own agent in a task left to a person", async () => {
        withOwnAgent()
        const task = await created({ title: 'By hand' })
        runner.leaveToPerson(task.id)
        await withWorkspace(task.id)
        tasks.applyEvent(task.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'ready', stage: 'build' },
        })
        await runner.settle()
        tasks.applyEvent(task.id, {
          source: 'human',
          type: 'state.overridden',
          payload: { to: 'running' },
        })
        await runner.settle()
        expect(tasks.getTask(task.id).lanes).toEqual({
          implement: 'running',
          'test-spec': 'running',
        })

        await restart()

        expect(terminals.list(task.id)).toEqual([])
      })

      it.each([
        ['Start', (id: string) => runner.act(id, 'start')],
        ['Start agent', (id: string) => runner.startByHand(id, 'claude-code')],
      ])('is no longer left to a person after %s', async (_name, start) => {
        const task = await created({ title: 'By hand', agent: 'claude-code' })
        runner.leaveToPerson(task.id)
        expect(tasks.getTask(task.id).withoutAgent).toBe(true)
        await withWorkspace(task.id)
        expect(terminals.list(task.id)).toEqual([])

        await start(task.id)
        await runner.settle()

        expect(tasks.getTask(task.id).withoutAgent).toBeUndefined()
        expect(tasks.getTask(task.id).state).toBe('running')
      })
    })
  })
})

describe('shells that come back after a restart', () => {
  let shells: OpenShells | undefined

  afterEach(() => {
    shells?.stop()
    shells = undefined
    vi.restoreAllMocks()
  })

  // The daemon starts again, with this list of the task's shells. The old shells are gone.
  function restart(taskId: string, list: OpenShell[]): OpenShells {
    terminals.close(taskId)
    runner.stop()
    saveOpenShells(dataDir, taskId, list)
    shells = new OpenShells({ dataDir, terminals, bus, isEnded: (id) => isTaskEnded(tasks, id) })
    shells.start()
    runner = makeRunner(agents, shells)
    runner.start()
    runner.loadOpenShells()
    return shells
  }

  // The spec runs in Shell 1.
  async function specRunning(agent = 'claude-code'): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent })
    await withWorkspace(task.id)
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'running' })
    return tasks.getTask(task.id)
  }

  const shellsOfT0075: OpenShell[] = [
    { name: '1', agent: 'claude-code', session: 's-1' },
    { name: '6', agent: 'copilot', session: 'c-1' },
    { name: '7', agent: null, session: null },
  ]

  it('lets every shell wait for a dashboard, except the shell of a working stage', async () => {
    const task = await specRunning()

    const open = restart(task.id, shellsOfT0075)

    expect(open.waiting(task.id)).toEqual(['6', '7'])
    expect(open.restoring(task.id)).toEqual({ '6': 'copilot', '7': null })
  })

  it('brings an agent back on its session once, and keeps the task agent', async () => {
    const task = await specRunning()
    const open = restart(task.id, shellsOfT0075)
    const before = ptys.length

    runner.restoreTask(task.id, '6')
    runner.restoreTask(task.id, '6')

    expect(ptys).toHaveLength(before + 1)
    expect(launchScript(task.id, '6')).toMatch(/^copilot /)
    expect(launchScript(task.id, '6')).toContain("--resume 'c-1'")
    expect(launchScript(task.id, '6')).not.toContain('Export tasks')
    expect(tasks.getTask(task.id).agent).toBe('claude-code')
    expect(open.restoring(task.id)).toEqual({ '6': 'copilot', '7': null })
    expect(runner.agentRuns(task.id, '6')).toBe(true)
  })

  it('brings back every agent of the task when any shell of it is attached', async () => {
    const task = await specRunning()
    const open = restart(task.id, [
      { name: '6', agent: 'copilot', session: 'c-6' },
      { name: '7', agent: null, session: null },
      { name: '8', agent: 'codex', session: 'x-8' },
    ])

    // The agents grid attaches Shell 1 only.
    runner.restoreTask(task.id, '1')

    expect(launchScript(task.id, '6')).toContain("--resume 'c-6'")
    expect(launchScript(task.id, '8')).toMatch(/^codex resume .*'x-8'/)
    // A plain shell starts with its own attach.
    expect(terminals.list(task.id)).not.toContain('7')
    expect(open.waiting(task.id)).toEqual(['7'])
  })

  it('brings back an agent with no session on the newest session of the folder', async () => {
    const task = await specRunning()
    restart(task.id, [{ name: '6', agent: 'copilot', session: null }])

    runner.restoreTask(task.id, '6')

    expect(launchScript(task.id, '6')).toMatch(/^copilot .*--continue/)
    expect(launchScript(task.id, '6')).not.toContain('Export tasks')
  })

  it('leaves the shell to the attach when its agent cannot start', async () => {
    const task = await specRunning()
    const open = restart(task.id, shellsOfT0075)
    vi.spyOn(agents, 'start').mockImplementation(() => {
      throw new Error('At most 30 terminals can be open')
    })
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    expect(() => runner.restoreTask(task.id, '6')).not.toThrow()

    expect(open.restoring(task.id)).toEqual({ '7': null })
    expect(warn).toHaveBeenCalledOnce()
  })

  it('lets a shell that was not attached wait again when its agent cannot start', async () => {
    const task = await specRunning()
    const open = restart(task.id, shellsOfT0075)
    vi.spyOn(agents, 'start').mockImplementation(() => {
      throw new Error('At most 30 terminals can be open')
    })
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    runner.restoreTask(task.id, '7')

    expect(open.restoring(task.id)).toEqual({ '6': 'copilot', '7': null })
  })

  it('keeps no shells of a task without a workspace', async () => {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })

    const open = restart(task.id, shellsOfT0075)

    expect(open.waiting(task.id)).toEqual([])
    expect(existsSync(openShellsPath(dataDir, task.id))).toBe(false)
  })

  it('forgets the shells of a task that ends', async () => {
    const task = await specRunning()
    restart(task.id, shellsOfT0075)

    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })
    await runner.settle()

    expect(existsSync(openShellsPath(dataDir, task.id))).toBe(false)
  })

  it('shows the agent of a working stage as coming back while the daemon starts it', async () => {
    const task = await specRunning()
    const open = restart(task.id, shellsOfT0075)

    runner.resumeAgents()
    await runner.settle()

    // The events have no session of the stage; the shell's list has the last one.
    expect(launchScript(task.id)).toContain("--resume 's-1'")
    expect(open.restoring(task.id)).toEqual({ '1': 'claude-code', '6': 'copilot', '7': null })
  })

  it('goes on with the session of the shell for an agent other than Claude Code', async () => {
    const task = await specRunning('codex')
    restart(task.id, [{ name: '1', agent: 'codex', session: 'x-1' }])

    runner.resumeAgents()
    await runner.settle()

    expect(launchScript(task.id)).toMatch(/^codex resume .*'x-1'/)
    expect(launchScript(task.id)).not.toContain('Export tasks')
  })

  it('takes the session of the shell over an older one in the events', async () => {
    const task = await specRunning()
    tasks.applyEvent(task.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 'old' },
    })
    expect(tasks.getTask(task.id).state).toBe('needs-input')
    restart(task.id, [{ name: '1', agent: 'claude-code', session: 'new' }])

    runner.resumeAgents()
    await runner.settle()

    expect(launchScript(task.id)).toContain("--resume 'new'")
  })

  it('leaves the agent of a working stage quit when the person quit it', async () => {
    const task = await specRunning()
    restart(task.id, [{ name: '1', agent: null, session: null }])
    const before = ptys.length

    runner.resumeAgents()
    await runner.settle()

    expect(ptys).toHaveLength(before)
    expect(tasks.getTask(task.id).state).toBe('running')
  })

  it('lets the shell of a stage wait when its agent cannot start again', async () => {
    const task = await specRunning()
    const open = restart(task.id, shellsOfT0075)
    vi.spyOn(agents, 'start').mockImplementation(() => {
      throw new Error('At most 30 terminals can be open')
    })

    runner.resumeAgents()
    await runner.settle()

    expect(open.waiting(task.id).sort()).toEqual(['1', '6', '7'])
    expect(readOpenShells(dataDir, task.id)).toContainEqual(shellsOfT0075[0])
  })
})

describe('the agent of a finished task, reopened', () => {
  let shells: OpenShells

  beforeEach(() => {
    runner.stop()
    shells = new OpenShells({ dataDir, terminals, bus, isEnded: (id) => isTaskEnded(tasks, id) })
    shells.start()
    runner = makeRunner(agents, shells)
    runner.start()
  })

  afterEach(() => {
    shells.stop()
  })

  // The spec agent works in Terminal 1.
  async function specRunning(): Promise<Task> {
    const task = await created({ title: 'Export tasks', agent: 'claude-code' })
    await withWorkspace(task.id)
    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'running' })
    return tasks.getTask(task.id)
  }

  async function end(taskId: string, how: 'done' | 'cancelled' = 'cancelled'): Promise<Task> {
    tasks.applyEvent(
      taskId,
      how === 'done'
        ? { source: 'human', type: 'state.overridden', payload: { to: 'done' } }
        : { source: 'human', type: 'task.cancelled' },
    )
    await runner.settle()
    return tasks.getTask(taskId)
  }

  // The spec agent reported `session` in Terminal 1, then the task ended.
  async function endedWith(session: string, how?: 'done' | 'cancelled'): Promise<Task> {
    const task = await specRunning()
    shells.noteSession(task.id, '1', 'claude-code', session)
    return end(task.id, how)
  }

  const lastLaunch = (taskId: string) =>
    tasks
      .listEvents(taskId)
      .map(({ event }) => event)
      .filter((event) => event.type === 'agent.launched')
      .at(-1)

  it('keeps the session of the agent in Terminal 1 when the task ends', async () => {
    const task = await endedWith('s-9')

    expect(readEndedShells(dataDir, task.id)[0]).toMatchObject({
      name: '1',
      agent: 'claude-code',
      stage: 'spec',
      session: 's-9',
    })
  })

  it('starts the agent again in Terminal 1 on its session, and the task stays cancelled', async () => {
    const task = await endedWith('s-9')
    expect(terminals.list(task.id)).toEqual([])
    const before = ptys.length

    const reopened = runner.reopenAgent(task.id)
    await runner.settle()

    expect(reopened).toMatchObject({ state: 'cancelled', agent: 'claude-code' })
    expect(tasks.getTask(task.id)).toMatchObject({ state: 'cancelled', agent: 'claude-code' })
    expect(ptys).toHaveLength(before + 1)
    expect(terminals.list(task.id)).toEqual(['1'])
    expect(launchScript(task.id)).toContain("--resume 's-9'")
    expect(launchScript(task.id)).not.toContain('Export tasks')
    expect(lastLaunch(task.id)?.payload).toMatchObject({
      agent: 'claude-code',
      terminal: '1',
      restored: true,
    })
    expect(shells.restoring(task.id)).toEqual({ '1': 'claude-code' })
    expect(existsSync(openShellsPath(dataDir, task.id))).toBe(false)
  })

  it('keeps a done task done', async () => {
    const task = await endedWith('s-9', 'done')
    expect(task.state).toBe('done')

    expect(runner.reopenAgent(task.id).state).toBe('done')
    expect(launchScript(task.id)).toContain("--resume 's-9'")
  })

  it('goes on with the newest session of the folder when none is known', async () => {
    const task = await end((await specRunning()).id)
    expect(readEndedShells(dataDir, task.id)[0]?.session).toBeNull()

    runner.reopenAgent(task.id)

    expect(launchScript(task.id)).toContain('--continue')
    expect(launchScript(task.id)).not.toContain('Export tasks')
  })

  it('reopens the task agent on its last session of the stage when no screen was kept', async () => {
    const running = await specRunning()
    tasks.applyEvent(running.id, {
      source: 'agent',
      type: 'agent.finished',
      payload: { stage: 'spec', sessionId: 's-4' },
    })
    // As after a restart: no shell was open when the task was cancelled.
    terminals.close(running.id)
    const task = await end(running.id)
    expect(readEndedShells(dataDir, task.id)).toEqual([])

    runner.reopenAgent(task.id)

    expect(launchScript(task.id)).toContain("--resume 's-4'")
  })

  it('keeps the newer session of the reopened agent for the next reopen', async () => {
    const task = await endedWith('s-9')
    runner.reopenAgent(task.id)

    shells.noteSession(task.id, '1', 'claude-code', 's-10')
    expect(readEndedShells(dataDir, task.id)[0]?.session).toBe('s-10')
    expect(existsSync(openShellsPath(dataDir, task.id))).toBe(false)

    // The agent quit and its terminal closed.
    terminals.close(task.id)
    runner.reopenAgent(task.id)
    expect(launchScript(task.id)).toContain("--resume 's-10'")
  })

  it('types the agent into a plain Terminal 1 that is open already', async () => {
    const task = await endedWith('s-9')
    // Open a new terminal: the dashboard attached a plain Terminal 1.
    terminals.run(task.id, 'ls')
    const plain = ptys.at(-1) as FakePty
    const before = ptys.length

    runner.reopenAgent(task.id)

    expect(ptys).toHaveLength(before)
    expect(terminals.list(task.id)).toEqual(['1'])
    expect(plain.written.join('')).toContain(`agent-launch-${task.id}.sh`)
  })

  it('refuses a task that is not finished, and Terminal 1 where an agent runs', async () => {
    const running = await specRunning()
    expect(() => runner.reopenAgent(running.id)).toThrow(StageActionError)

    const task = await end(running.id)
    runner.reopenAgent(task.id)
    const before = ptys.length
    const written = ptys.at(-1)?.written.length

    expect(() => runner.reopenAgent(task.id)).toThrow(StageActionError)
    expect(ptys).toHaveLength(before)
    expect(ptys.at(-1)?.written).toHaveLength(written ?? 0)
  })

  it('refuses a task with no workspace or no agent', async () => {
    const bare = await created({ title: 'Export tasks', agent: 'claude-code' })
    await end(bare.id)
    expect(() => runner.reopenAgent(bare.id)).toThrow(StageActionError)

    // Held at the context gate, so no agent ever started.
    const task = await created({ title: 'Import tasks' })
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'context' },
    })
    await withWorkspace(task.id)
    expect((await end(task.id)).agent).toBeNull()
    const before = ptys.length

    expect(() => runner.reopenAgent(task.id)).toThrow(StageActionError)
    expect(ptys).toHaveLength(before)
  })

  it('refuses a task that was deleted, and keeps no file of it', async () => {
    const task = await endedWith('s-9')
    await workspaces.remove(task.id, { deleteBranch: false })
    tasks.deleteTask(task.id)
    await runner.settle()

    expect(() => runner.reopenAgent(task.id)).toThrow()
    expect(existsSync(join(dataDir, 'tasks', `${task.id}.shells.json`))).toBe(false)
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

describe('a parent with subtasks', () => {
  beforeEach(() => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'implement', kind: 'agent', gate: 'manual' },
        { id: 'review', kind: 'review', gate: 'manual' },
      ],
    }
  })

  // A parent whose Implement waits to start in its workspace, and one draft subtask.
  async function waitingParent(): Promise<{ parent: Task; child: Task }> {
    const task = await created({ title: 'Export', agent: 'claude-code' })
    const child = tasks.createTask({ title: 'Part A', parentId: task.id })
    await withWorkspace(task.id)
    const parent = await runner.act(task.id, 'start')
    await runner.settle()
    return { parent: tasks.getTask(parent.id), child }
  }

  async function finish(id: string): Promise<void> {
    tasks.applyEvent(id, { source: 'human', type: 'state.overridden', payload: { to: 'done' } })
    await runner.settle()
  }

  // The remote copy of the parent's branch got a merged subtask.
  function mergedOnRemote(parent: Task): void {
    const origin = addBareOrigin(repo)
    runGit(parent.workspace ?? '', 'push', '-u', 'origin', parent.branch ?? '')
    const other = join(repo.root, 'other')
    runGit(repo.root, 'clone', '--branch', parent.branch ?? '', origin, other)
    runGit(other, 'commit', '--allow-empty', '-m', 'merged subtask')
    runGit(other, 'push', 'origin', parent.branch ?? '')
  }

  it('starts a new stage once its open subtasks are done', async () => {
    const { parent, child } = await waitingParent()
    expect(parent).toMatchObject({ stage: 'implement', state: 'draft' })
    expect(ptys).toHaveLength(0)

    await finish(child.id)

    expect(tasks.getTask(parent.id).state).toBe('running')
    expect(ptys).toHaveLength(1)
  })

  it('stops waiting when its subtask is cancelled or deleted', async () => {
    const { parent, child } = await waitingParent()
    const other = tasks.createTask({ title: 'Part B', parentId: parent.id })

    tasks.applyEvent(child.id, { source: 'human', type: 'task.cancelled' })
    await runner.settle()
    expect(tasks.getTask(parent.id).state).toBe('draft')

    tasks.deleteTask(other.id)
    await runner.settle()
    expect(tasks.getTask(parent.id).state).toBe('running')
  })

  it('does not stop a stage that runs when a subtask is added', async () => {
    const task = await created({ title: 'Export', agent: 'claude-code' })
    await withWorkspace(task.id)
    await runner.act(task.id, 'start')
    await runner.settle()
    expect(tasks.getTask(task.id).state).toBe('running')
    tasks.createTask({ title: 'Part A', parentId: task.id })
    await runner.settle()

    expect(tasks.getTask(task.id).state).toBe('running')
    expect(ptys[0]?.killed).not.toBe(true)
  })

  it('pulls the merged work of its subtasks before the stage starts', async () => {
    const { parent, child } = await waitingParent()
    mergedOnRemote(parent)

    await finish(child.id)

    expect(tasks.getTask(parent.id).state).toBe('running')
    expect(runGit(parent.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('merged subtask')
  })

  it('fails the stage when the pull fails', async () => {
    const { parent, child } = await waitingParent()
    mergedOnRemote(parent)
    runGit(parent.workspace ?? '', 'commit', '--allow-empty', '-m', 'local only')

    await finish(child.id)

    const failed = tasks
      .listEvents(parent.id)
      .find((entry) => entry.event.type === 'stage.check-failed')
    expect(String(failed?.event.payload.reasons)).toContain(
      'Could not pull the work of the subtasks',
    )
    expect(ptys).toHaveLength(0)
  })
})
