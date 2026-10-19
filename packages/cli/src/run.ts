import { DaemonClient, type FetchFunction } from './client.js'
import { type Command, parseCommand, UsageError } from './command.js'
import { type CliIo, execute, printJson } from './execute.js'
import { DEFAULT_URL, USAGE } from './format.js'
import { runHookCommand } from './forward-hook.js'
import { claudeCodeHookSettings } from './hooks.js'

export type { CliIo }

export type CliOptions = {
  env: Record<string, string | undefined>
  io: CliIo
  fetch?: FetchFunction
  // Where `natamux hook` reads the JSON of the agent's hook. Defaults to an empty input.
  readStdin?: () => Promise<string>
}

export const EXIT_OK = 0
export const EXIT_FAILED = 1
export const EXIT_USAGE = 2

// Returns the exit code: 0 done, 1 the request failed, 2 the command line is wrong.
export async function runCli(argv: readonly string[], options: CliOptions): Promise<number> {
  const { env, io } = options

  // Same code as the fast path in main.ts, which never loads this module.
  if (argv[0] === 'hook') {
    return runHookCommand(argv.slice(1), {
      env,
      stdout: io.stdout,
      stderr: io.stderr,
      readStdin: options.readStdin ?? (async () => ''),
      fetch: options.fetch ?? fetch,
    })
  }

  let command: Command
  try {
    command = parseCommand(argv)
  } catch (error) {
    if (error instanceof UsageError) {
      io.stderr(`${error.message}\nRun "natamux help" for usage.\n`)
      return EXIT_USAGE
    }
    throw error
  }

  if (command.name === 'help') {
    io.stdout(USAGE)
    return EXIT_OK
  }

  // This command only prints settings, so it needs no token and does not call the daemon.
  if (command.name === 'hooks.claude-code') {
    const settings = claudeCodeHookSettings(env.NATAMUX_URL?.trim() || DEFAULT_URL)
    printJson(io, settings)
    return EXIT_OK
  }

  // `ask`, `done` and `task show` run inside a task's terminal, which knows its task.
  if (
    (command.name === 'agent.ask' ||
      command.name === 'agent.done' ||
      command.name === 'task.show') &&
    command.id === undefined
  ) {
    const fromEnv = env.NATAMUX_TASK_ID?.trim()
    if (!fromEnv) {
      io.stderr(
        command.name === 'task.show'
          ? 'Name the task, for example natamux task show T-42, or set NATAMUX_TASK_ID.\n'
          : 'Name the task with --task T-42, or set NATAMUX_TASK_ID.\n',
      )
      return EXIT_USAGE
    }
    command = { ...command, id: fromEnv }
  }

  const token = env.NATAMUX_TOKEN?.trim()
  if (!token) {
    io.stderr('NATAMUX_TOKEN is not set. Use the token the daemon printed when it started.\n')
    return EXIT_USAGE
  }

  const client = new DaemonClient({
    baseUrl: env.NATAMUX_URL?.trim() || DEFAULT_URL,
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
