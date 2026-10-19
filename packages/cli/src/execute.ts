import { readFile, stat } from 'node:fs/promises'
import { basename } from 'node:path'
import { type AttachmentUpload, attachmentLimitError } from '@natamux/core'
import type { DaemonClient } from './client.js'
import type { Command } from './command.js'
import {
  formatEventLog,
  formatEventResult,
  formatTask,
  formatTaskList,
  formatWorkspaceCreated,
  formatWorkspaceRemoved,
  USAGE,
} from './format.js'

type Env = Record<string, string | undefined>

export type CliIo = {
  stdout: (text: string) => void
  stderr: (text: string) => void
}

type Context = { client: DaemonClient; io: CliIo; env: Env }
type Of<Name extends Command['name']> = Extract<Command, { name: Name }>

export function printJson(io: CliIo, data: unknown): void {
  io.stdout(`${JSON.stringify(data, null, 2)}\n`)
}

// The text is built only when the command does not ask for JSON.
function output(io: CliIo, json: boolean, data: unknown, text: () => string): void {
  if (json) {
    printJson(io, data)
  } else {
    io.stdout(text())
  }
}

// The terminal the command runs in, when it is a terminal of that task. Inside a group it tells
// the daemon which stage the agent there works on.
function terminalOf(env: Env, taskId: string): string | undefined {
  const terminal = env.NATAMUX_TERMINAL?.trim()
  return terminal && env.NATAMUX_TASK_ID?.trim() === taskId ? terminal : undefined
}

// The project of a new task that names none: the one of the task this terminal belongs to, or
// the only project. The daemon would pick the "default" project, which may not exist.
async function projectOf(client: DaemonClient, env: Env): Promise<string | null> {
  const taskId = env.NATAMUX_TASK_ID?.trim()
  if (taskId) {
    return (await client.getTask(taskId)).repo
  }
  const repos = await client.listRepos()
  const [only] = repos
  if (only !== undefined && repos.length === 1) {
    return only.id
  }
  if (repos.length === 0) {
    throw new Error('There is no project. Add a project first')
  }
  throw new Error(
    `Name the project with --repo. Projects: ${repos.map((repo) => repo.id).join(', ')}`,
  )
}

// Every file is checked before the first is read and before the task is made, so a missing file
// or one over a limit makes no task.
async function readAttachments(paths: readonly string[]): Promise<AttachmentUpload[]> {
  const files = await Promise.all(
    paths.map(async (path) => {
      const info = await stat(path).catch(() => undefined)
      if (info === undefined || !info.isFile()) {
        throw new Error(`There is no file at ${path}`)
      }
      return { path, name: basename(path), size: info.size }
    }),
  )
  const error = attachmentLimitError(files)
  if (error !== undefined) {
    throw new Error(error)
  }
  return Promise.all(
    files.map(async ({ path, name }) => ({
      name,
      data: (await readFile(path)).toString('base64'),
    })),
  )
}

async function createTask(command: Of<'task.create'>, { client, io, env }: Context) {
  const attachments = await readAttachments(command.attach ?? [])
  // A subtask has no repo of its own unless named: the daemon uses the parent's project.
  const repo =
    command.repo ?? (command.parentId === undefined ? await projectOf(client, env) : undefined)
  const task = await client.createTask({
    title: command.title,
    description: command.description,
    priority: command.priority,
    labels: command.labels,
    agent: command.agent,
    dependsOn: command.dependsOn,
    ...(command.context === undefined ? {} : { context: command.context }),
    ...(attachments.length === 0 ? {} : { attachments }),
    repo,
    ...(command.parentId === undefined ? {} : { parentId: command.parentId }),
  })
  output(io, command.json, task, () => `Created ${task.id}  ${task.title}\n`)
}

async function listTasks(command: Of<'task.list'>, { client, io }: Context) {
  const tasks = await client.listTasks(command.state)
  output(io, command.json, tasks, () => `${formatTaskList(tasks)}\n`)
}

async function showTask(command: Of<'task.show'>, { client, io }: Context) {
  const task = await client.getTask(command.id ?? '')
  output(io, command.json, task, () => `${formatTask(task)}\n`)
}

async function sendEvent(command: Of<'event.send'>, { client, io }: Context) {
  const result = await client.sendEvent(command.id, {
    source: command.source,
    type: command.type,
    payload: command.payload,
  })
  output(io, command.json, result, () => `${formatEventResult(result)}\n`)
}

async function listEvents(command: Of<'event.list'>, { client, io }: Context) {
  const events = await client.listEvents(command.id)
  output(io, command.json, events, () => `${formatEventLog(events)}\n`)
}

async function editTask(command: Of<'task.edit'>, { client, io }: Context) {
  const task = await client.updateTask(command.id, command.changes)
  output(io, command.json, task, () => `${formatTask(task)}\n`)
}

async function startAgent(command: Of<'agent.start'>, { client, io }: Context) {
  const task = await client.startAgent(command.id, command.agent, command.prompt, command.base)
  output(io, command.json, task, () => `Started ${command.agent} in the terminal of ${task.id}\n`)
}

async function reportDone(command: Of<'agent.done'>, { client, io, env }: Context) {
  const id = command.id ?? ''
  const terminal = terminalOf(env, id)
  const task = await client.stageAction(id, {
    action: 'report',
    ...(command.stage === undefined ? {} : { stage: command.stage }),
    ...(terminal === undefined ? {} : { terminal }),
  })
  output(io, command.json, task, () => `Reported done; the checks run now.\n${formatTask(task)}\n`)
}

async function actOnStage(command: Of<'stage.act'>, { client, io, env }: Context) {
  const terminal = terminalOf(env, command.id)
  const task = await client.stageAction(command.id, {
    action: command.action === 'back' ? 'reopen' : command.action,
    ...(command.stage === undefined ? {} : { stage: command.stage }),
    ...(terminal === undefined ? {} : { terminal }),
    ...(command.to === undefined ? {} : { to: command.to }),
    ...(command.reason === undefined ? {} : { reason: command.reason }),
  })
  output(io, command.json, task, () => `${formatTask(task)}\n`)
}

async function setWorkflow(command: Of<'stage.workflow'>, { client, io }: Context) {
  const task = await client.setWorkflow(command.id, command.stage, command.on)
  output(io, command.json, task, () => {
    const how = command.on ? 'with' : 'without'
    return `${command.stage} runs ${how} a workflow the next time it starts.\n${formatTask(task)}\n`
  })
}

async function setCi(command: Of<'stage.ci'>, { client, io }: Context) {
  const task = await client.setCi(command.id, command.stage, command.on)
  output(io, command.json, task, () => {
    const how = command.on ? 'waits for CI' : 'does not wait for CI'
    return `${command.stage} ${how} before it passes.\n${formatTask(task)}\n`
  })
}

async function linkIssue(command: Of<'task.link'>, { client, io }: Context) {
  const task = await client.linkIssue(command.id, {
    provider: command.provider,
    key: command.key,
    url: command.url,
  })
  output(io, command.json, task, () => `${formatTask(task)}\n`)
}

async function askQuestion(command: Of<'agent.ask'>, { client, io, env }: Context) {
  const id = command.id ?? ''
  const terminal = terminalOf(env, id)
  const result = await client.sendEvent(id, {
    source: 'agent',
    type: 'agent.asked',
    payload: {
      question: command.question,
      ...(command.stage === undefined ? {} : { stage: command.stage }),
      ...(terminal === undefined ? {} : { terminal }),
    },
  })
  output(io, command.json, result, () => `${formatEventResult(result)}\n`)
}

async function createWorkspace(command: Of<'workspace.create'>, { client, io }: Context) {
  const task = await client.createWorkspace(command.id, command.base)
  output(io, command.json, task, () => `${formatWorkspaceCreated(task)}\n`)
}

async function removeWorkspace(command: Of<'workspace.remove'>, { client, io }: Context) {
  const result = await client.removeWorkspace(command.id, command.deleteBranch)
  output(io, command.json, result, () => `${formatWorkspaceRemoved(result)}\n`)
}

export async function execute(
  command: Command,
  client: DaemonClient,
  io: CliIo,
  env: Env,
): Promise<void> {
  const context: Context = { client, io, env }
  switch (command.name) {
    case 'help':
      io.stdout(USAGE)
      return
    case 'task.create':
      return createTask(command, context)
    case 'task.list':
      return listTasks(command, context)
    case 'task.show':
      return showTask(command, context)
    case 'event.send':
      return sendEvent(command, context)
    case 'event.list':
      return listEvents(command, context)
    case 'task.edit':
      return editTask(command, context)
    case 'agent.start':
      return startAgent(command, context)
    case 'agent.done':
      return reportDone(command, context)
    case 'stage.act':
      return actOnStage(command, context)
    case 'stage.workflow':
      return setWorkflow(command, context)
    case 'stage.ci':
      return setCi(command, context)
    case 'task.link':
      return linkIssue(command, context)
    case 'agent.ask':
      return askQuestion(command, context)
    case 'workspace.create':
      return createWorkspace(command, context)
    case 'workspace.remove':
      return removeWorkspace(command, context)
  }
}
