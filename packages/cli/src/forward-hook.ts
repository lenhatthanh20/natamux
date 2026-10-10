import { TASK_ID_HEADER, TERMINAL_HEADER } from '@natamux/core'
import type { FetchFunction } from './client.js'
import { DEFAULT_URL } from './format.js'

const REQUEST_TIMEOUT_MS = 3000

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
