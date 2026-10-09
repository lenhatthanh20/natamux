import { TASK_ID_HEADER, TERMINAL_HEADER } from '@helmsman/core'
import type { FetchFunction } from './client.js'
import { DEFAULT_URL } from './format.js'

const REQUEST_TIMEOUT_MS = 3000

type ForwardOptions = {
  agent: string
  env: Record<string, string | undefined>
  readStdin: () => Promise<string>
  fetch: FetchFunction
  debug: (text: string) => void
}

// Sends the JSON of an agent's hook to the daemon. It runs inside the agent, so it must never
// make the agent fail or print anything: any problem ends silently (HELMSMAN_DEBUG shows it).
// Without HELMSMAN_TOKEN the session is not one of Helmsman's, so it does nothing at all.
// Node's fetch does not use the proxy variables, so a proxy on the machine cannot block it.
export async function forwardHook(options: ForwardOptions): Promise<void> {
  const { agent, env } = options
  const token = env.HELMSMAN_TOKEN?.trim()
  if (!token) {
    return
  }

  try {
    const body = await options.readStdin()
    if (body.trim() === '') {
      return
    }

    const headers: Record<string, string> = {
      authorization: `Bearer ${token}`,
      'content-type': 'application/json',
    }
    const taskId = env.HELMSMAN_TASK_ID?.trim()
    if (taskId) {
      headers[TASK_ID_HEADER] = taskId
    }
    const terminal = env.HELMSMAN_TERMINAL?.trim()
    if (terminal) {
      headers[TERMINAL_HEADER] = terminal
    }

    const baseUrl = (env.HELMSMAN_URL?.trim() || DEFAULT_URL).replace(/\/+$/, '')
    const response = await options.fetch(`${baseUrl}/hooks/${encodeURIComponent(agent)}`, {
      method: 'POST',
      headers,
      body,
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    })
    if (!response.ok && env.HELMSMAN_DEBUG) {
      options.debug(`helmsman hook ${agent}: the daemon answered ${response.status}\n`)
    }
  } catch (error) {
    if (env.HELMSMAN_DEBUG) {
      const reason = error instanceof Error ? error.message : String(error)
      options.debug(`helmsman hook ${agent}: ${reason}\n`)
    }
  }
}
