import { DaemonClient, type FetchFunction } from './client.js'
import { type Command, parseCommand, UsageError } from './command.js'
import {
  DEFAULT_URL,
  formatEventLog,
  formatEventResult,
  formatTask,
  formatTaskList,
  formatWorkspaceCreated,
  formatWorkspaceRemoved,
  USAGE,
} from './format.js'
import { forwardHook } from './forward-hook.js'
import { claudeCodeHookSettings } from './hooks.js'

export type CliIo = {
  stdout: (text: string) => void
  stderr: (text: string) => void
}

export type CliOptions = {
  env: Record<string, string | undefined>
  io: CliIo
  fetch?: FetchFunction
  // Where `helmsman hook` reads the JSON of the agent's hook. Defaults to an empty input.
  readStdin?: () => Promise<string>
}

export const EXIT_OK = 0
export const EXIT_FAILED = 1
export const EXIT_USAGE = 2

function printJson(io: CliIo, data: unknown): void {
  io.stdout(`${JSON.stringify(data, null, 2)}\n`)
}

// The terminal the command runs in, when it is a terminal of that task. Inside a group it tells
// the daemon which stage the agent there works on.
function terminalOf(env: Record<string, string | undefined>, taskId: string): string | undefined {
  const terminal = env.HELMSMAN_TERMINAL?.trim()
  return terminal && env.HELMSMAN_TASK_ID?.trim() === taskId ? terminal : undefined
}

async function execute(
  command: Command,
  client: DaemonClient,
  io: CliIo,
  env: Record<string, string | undefined>,
): Promise<void> {
  switch (command.name) {
    case 'help':
      io.stdout(USAGE)
      return

    case 'task.create': {
      const task = await client.createTask({
        title: command.title,
        description: command.description,
        priority: command.priority,
        labels: command.labels,
        agent: command.agent,
        dependsOn: command.dependsOn,
        ...(command.context === undefined ? {} : { context: command.context }),
      })
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`Created ${task.id}  ${task.title}\n`)
      }
      return
    }

    case 'task.list': {
      const tasks = await client.listTasks(command.state)
      if (command.json) {
        printJson(io, tasks)
      } else {
        io.stdout(`${formatTaskList(tasks)}\n`)
      }
      return
    }

    case 'task.show': {
      const task = await client.getTask(command.id)
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`${formatTask(task)}\n`)
      }
      return
    }

    case 'event.send': {
      const result = await client.sendEvent(command.id, {
        source: command.source,
        type: command.type,
        payload: command.payload,
      })
      if (command.json) {
        printJson(io, result)
      } else {
        io.stdout(`${formatEventResult(result)}\n`)
      }
      return
    }

    case 'event.list': {
      const events = await client.listEvents(command.id)
      if (command.json) {
        printJson(io, events)
      } else {
        io.stdout(`${formatEventLog(events)}\n`)
      }
      return
    }

    case 'task.edit': {
      const task = await client.updateTask(command.id, command.changes)
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`${formatTask(task)}\n`)
      }
      return
    }

    case 'agent.start': {
      const task = await client.startAgent(command.id, command.agent, command.prompt)
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`Started ${command.agent} in the terminal of ${task.id}\n`)
      }
      return
    }

    case 'agent.done': {
      const id = command.id ?? ''
      const terminal = terminalOf(env, id)
      const task = await client.stageAction(id, {
        action: 'report',
        ...(command.stage === undefined ? {} : { stage: command.stage }),
        ...(terminal === undefined ? {} : { terminal }),
      })
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`Reported done; the checks run now.\n${formatTask(task)}\n`)
      }
      return
    }

    case 'stage.act': {
      const terminal = terminalOf(env, command.id)
      const task = await client.stageAction(command.id, {
        action: command.action === 'back' ? 'reopen' : command.action,
        ...(command.stage === undefined ? {} : { stage: command.stage }),
        ...(terminal === undefined ? {} : { terminal }),
        ...(command.to === undefined ? {} : { to: command.to }),
        ...(command.reason === undefined ? {} : { reason: command.reason }),
      })
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`${formatTask(task)}\n`)
      }
      return
    }

    case 'task.link': {
      const task = await client.linkIssue(command.id, {
        provider: command.provider,
        key: command.key,
        url: command.url,
      })
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`${formatTask(task)}\n`)
      }
      return
    }

    case 'agent.ask': {
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
      if (command.json) {
        printJson(io, result)
      } else {
        io.stdout(`${formatEventResult(result)}\n`)
      }
      return
    }

    case 'workspace.create': {
      const task = await client.createWorkspace(command.id, command.base)
      if (command.json) {
        printJson(io, task)
      } else {
        io.stdout(`${formatWorkspaceCreated(task)}\n`)
      }
      return
    }

    case 'workspace.remove': {
      const result = await client.removeWorkspace(command.id, command.deleteBranch)
      if (command.json) {
        printJson(io, result)
      } else {
        io.stdout(`${formatWorkspaceRemoved(result)}\n`)
      }
      return
    }
  }
}

// Returns the exit code: 0 done, 1 the request failed, 2 the command line is wrong.
export async function runCli(argv: readonly string[], options: CliOptions): Promise<number> {
  const { env, io } = options

  let command: Command
  try {
    command = parseCommand(argv)
  } catch (error) {
    if (error instanceof UsageError) {
      io.stderr(`${error.message}\nRun "helmsman help" for usage.\n`)
      // `helmsman hook` runs inside an agent, which treats exit code 2 as "block this action".
      return argv[0] === 'hook' ? EXIT_OK : EXIT_USAGE
    }
    throw error
  }

  if (command.name === 'help') {
    io.stdout(USAGE)
    return EXIT_OK
  }

  // Always exit 0 and print nothing: it runs inside an agent and must never get in its way.
  if (command.name === 'hook.forward') {
    await forwardHook({
      agent: command.agent,
      env,
      readStdin: options.readStdin ?? (async () => ''),
      fetch: options.fetch ?? fetch,
      debug: io.stderr,
    })
    return EXIT_OK
  }

  // This command only prints settings, so it needs no token and does not call the daemon.
  if (command.name === 'hooks.claude-code') {
    const settings = claudeCodeHookSettings(env.HELMSMAN_URL?.trim() || DEFAULT_URL)
    printJson(io, settings)
    return EXIT_OK
  }

  // `ask` and `done` run inside an agent's terminal, which knows its task.
  if ((command.name === 'agent.ask' || command.name === 'agent.done') && command.id === undefined) {
    const fromEnv = env.HELMSMAN_TASK_ID?.trim()
    if (!fromEnv) {
      io.stderr('Name the task with --task T-42, or set HELMSMAN_TASK_ID.\n')
      return EXIT_USAGE
    }
    command = { ...command, id: fromEnv }
  }

  const token = env.HELMSMAN_TOKEN?.trim()
  if (!token) {
    io.stderr('HELMSMAN_TOKEN is not set. Use the token the daemon printed when it started.\n')
    return EXIT_USAGE
  }

  const client = new DaemonClient({
    baseUrl: env.HELMSMAN_URL?.trim() || DEFAULT_URL,
    token,
    fetch: options.fetch ?? fetch,
  })

  try {
    await execute(command, client, io, env)
    return EXIT_OK
  } catch (error) {
    io.stderr(`Error: ${error instanceof Error ? error.message : String(error)}\n`)
    return EXIT_FAILED
  }
}
