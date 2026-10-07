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

export type CliIo = {
  stdout: (text: string) => void
  stderr: (text: string) => void
}

export type CliOptions = {
  env: Record<string, string | undefined>
  io: CliIo
  fetch?: FetchFunction
}

export const EXIT_OK = 0
export const EXIT_FAILED = 1
export const EXIT_USAGE = 2

function printJson(io: CliIo, data: unknown): void {
  io.stdout(`${JSON.stringify(data, null, 2)}\n`)
}

async function execute(command: Command, client: DaemonClient, io: CliIo): Promise<void> {
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

    case 'workspace.create': {
      const task = await client.createWorkspace(command.id)
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
      return EXIT_USAGE
    }
    throw error
  }

  if (command.name === 'help') {
    io.stdout(USAGE)
    return EXIT_OK
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
    await execute(command, client, io)
    return EXIT_OK
  } catch (error) {
    io.stderr(`Error: ${error instanceof Error ? error.message : String(error)}\n`)
    return EXIT_FAILED
  }
}
