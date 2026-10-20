import type { FetchFunction } from './client.js'
import { DEFAULT_URL } from './default-url.js'

// This module is all that `natamux hook` loads, so it must not import @natamux/core (it pulls in
// zod). A test checks these two against the core values.
export const TASK_ID_HEADER = 'X-Natamux-Task'
export const TERMINAL_HEADER = 'X-Natamux-Terminal'

const REQUEST_TIMEOUT_MS = 3000
const AGENT_NAME = /^[a-z][a-z0-9-]*$/
const EVENT_NAME = /^[A-Za-z][A-Za-z0-9_-]{0,63}$/
// Agents that read a hook's output as JSON.
const JSON_ANSWER_AGENTS: readonly string[] = ['cursor', 'gemini']

type ForwardOptions = {
  agent: string
  // Named by the hook command when the agent's JSON does not say it.
  event?: string
  env: Record<string, string | undefined>
  readStdin: () => Promise<string>
  fetch: FetchFunction
  debug: (text: string) => void
}

// Sends the JSON of an agent's hook to the daemon. It runs inside the agent, so it must never
// make the agent fail or print anything: any problem ends silently (NATAMUX_DEBUG shows it).
// Without NATAMUX_TOKEN the session is not one of Natamux's, so it does nothing at all.
// Node's fetch does not use the proxy variables, so a proxy on the machine cannot block it.
export async function forwardHook(options: ForwardOptions): Promise<void> {
  const { agent, env } = options
  const token = env.NATAMUX_TOKEN?.trim()
  if (!token) {
    return
  }

  try {
    // Cursor on Windows starts its JSON with a byte order mark, which JSON parsers refuse.
    const body = (await options.readStdin()).replace(/^\uFEFF/, '')
    if (body.trim() === '') {
      return
    }

    const headers: Record<string, string> = {
      authorization: `Bearer ${token}`,
      'content-type': 'application/json',
    }
    const taskId = env.NATAMUX_TASK_ID?.trim()
    if (taskId) {
      headers[TASK_ID_HEADER] = taskId
    }
    const terminal = env.NATAMUX_TERMINAL?.trim()
    if (terminal) {
      headers[TERMINAL_HEADER] = terminal
    }

    const baseUrl = (env.NATAMUX_URL?.trim() || DEFAULT_URL).replace(/\/+$/, '')
    const query = options.event === undefined ? '' : `?event=${encodeURIComponent(options.event)}`
    const response = await options.fetch(`${baseUrl}/hooks/${encodeURIComponent(agent)}${query}`, {
      method: 'POST',
      headers,
      body,
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    })
    if (!response.ok && env.NATAMUX_DEBUG) {
      options.debug(`natamux hook ${agent}: the daemon answered ${response.status}\n`)
    }
  } catch (error) {
    if (env.NATAMUX_DEBUG) {
      const reason = error instanceof Error ? error.message : String(error)
      options.debug(`natamux hook ${agent}: ${reason}\n`)
    }
  }
}

export type HookCommandOptions = {
  env: Record<string, string | undefined>
  stdout: (text: string) => void
  stderr: (text: string) => void
  readStdin: () => Promise<string>
  fetch: FetchFunction
}

// `natamux hook <agent> [event]`, with the arguments after "hook". It always exits 0: it runs
// inside an agent, which treats exit code 2 as "block this action". It prints an empty JSON answer
// for the agents that read one (Cursor blocks a shell command on an answer that is not JSON), and
// nothing for the others.
export async function runHookCommand(
  args: readonly string[],
  options: HookCommandOptions,
): Promise<number> {
  const [agent, event, ...extra] = args
  let problem: string | undefined
  if (agent === undefined || !AGENT_NAME.test(agent)) {
    problem =
      'An agent name is required, in lower case letters, digits and "-": natamux hook claude-code'
  } else if (extra.length > 0) {
    problem = `Unexpected argument: ${extra.join(' ')}`
  } else if (event !== undefined && !EVENT_NAME.test(event)) {
    problem = `Unexpected argument: ${event}`
  }
  if (agent === undefined || problem !== undefined) {
    options.stderr(`${problem}\nRun "natamux help" for usage.\n`)
    return 0
  }

  // Copilot CLI also runs the hooks in `.claude/settings.local.json`; only Claude Code sets
  // CLAUDECODE, so Copilot's events never reach the daemon as Claude Code's.
  if (agent === 'claude-code' && options.env.CLAUDECODE !== '1') {
    return 0
  }
  if (JSON_ANSWER_AGENTS.includes(agent)) {
    options.stdout('{}\n')
  }
  await forwardHook({
    agent,
    // Copilot CLI does not say which event a hook is for, so its hook command names it.
    ...(event === undefined ? {} : { event }),
    env: options.env,
    readStdin: options.readStdin,
    fetch: options.fetch,
    debug: options.stderr,
  })
  return 0
}
