import { readFileSync } from 'node:fs'
import { join } from 'node:path'

// The smoke check of CI, after the dashboard loaded: a task gets a worktree and a shell in it runs
// a command. When NATAMUX_CLAUDE_COMMAND names a stand-in agent, that agent must also run the
// app's own `natamux` command, which writes SMOKE_FILE in the worktree. Throws what failed.

export const SMOKE_FILE = 'natamux-smoke.txt'
const SHELL_TIMEOUT_MS = 30_000
const AGENT_TIMEOUT_MS = 60_000
// biome-ignore lint/suspicious/noControlCharactersInRegex: escape sequences start with ESC
const ESCAPES = /\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]/g

// A line whose output, 42xyz, never shows in the echo of the line itself.
export function smokeCommandLine(platform: NodeJS.Platform): string {
  return platform === 'win32' ? 'Write-Output "$(6*7)xyz"' : 'echo "$((6*7))xyz"'
}

// The Windows console may redraw the screen with cursor moves between the characters, so the
// escape sequences go first.
export function hasSmokeOutput(output: string): boolean {
  return output.replace(ESCAPES, '').includes('42xyz')
}

export async function runSmoke(
  dashboardUrl: string,
  token: string,
  env: NodeJS.ProcessEnv,
  platform: NodeJS.Platform,
): Promise<void> {
  const call = async (method: string, path: string, body?: unknown) => {
    const response = await fetch(`${dashboardUrl}${path}`, {
      method,
      headers: {
        authorization: `Bearer ${token}`,
        ...(body === undefined ? {} : { 'content-type': 'application/json' }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
    const text = await response.text()
    if (!response.ok) {
      throw new Error(`${method} ${path}: ${response.status} ${text}`)
    }
    return JSON.parse(text) as { task?: { id: string; workspace: string | null }; ticket?: string }
  }

  const withAgent = Boolean(env.NATAMUX_CLAUDE_COMMAND)
  const created = await call('POST', '/tasks', { title: 'Desktop smoke', agent: 'claude-code' })
  const id = created.task?.id ?? ''
  // Starting the agent makes the worktree first.
  if (withAgent) {
    await call('POST', `/tasks/${id}/agent`, { agent: 'claude-code', prompt: 'smoke' })
  } else {
    await call('POST', `/tasks/${id}/workspace`, {})
  }
  const workspace = (await call('GET', `/tasks/${id}`)).task?.workspace
  if (!workspace) {
    throw new Error(`${id} has no worktree`)
  }

  // Terminal 2, so the agent in the first one is left alone.
  const { ticket } = await call('POST', `/tasks/${id}/terminal/ticket`)
  const socketUrl = new URL(`/tasks/${id}/terminal`, dashboardUrl.replace(/^http/, 'ws'))
  const query = { ticket: ticket ?? '', terminal: '2', cols: '120', rows: '30' }
  socketUrl.search = new URLSearchParams(query).toString()
  await new Promise<void>((resolve, reject) => {
    const socket = new WebSocket(socketUrl)
    socket.binaryType = 'arraybuffer'
    const decoder = new TextDecoder()
    let output = ''
    let sent = false
    const end = (error?: Error) => {
      clearTimeout(timer)
      socket.close()
      if (error === undefined) {
        resolve()
      } else {
        reject(error)
      }
    }
    const timer = setTimeout(
      () => end(new Error(`the shell did not print 42xyz. It printed: ${JSON.stringify(output)}`)),
      SHELL_TIMEOUT_MS,
    )
    socket.addEventListener('message', (event) => {
      if (typeof event.data === 'string') {
        return
      }
      output += decoder.decode(event.data as ArrayBuffer, { stream: true })
      if (!sent) {
        // The first output is the prompt: the shell reads its input from now on.
        sent = true
        setTimeout(() => {
          socket.send(JSON.stringify({ type: 'input', data: `${smokeCommandLine(platform)}\r` }))
        }, 1000)
      } else if (hasSmokeOutput(output)) {
        end()
      }
    })
    socket.addEventListener('close', () => end(new Error('the terminal closed')))
    socket.addEventListener('error', () => end(new Error('the terminal socket failed')))
  })

  if (!withAgent) {
    return
  }
  const file = join(workspace, SMOKE_FILE)
  const deadline = Date.now() + AGENT_TIMEOUT_MS
  for (;;) {
    let text = ''
    try {
      text = readFileSync(file, 'utf8')
    } catch {
      // Not written yet.
    }
    if (text.includes(id)) {
      return
    }
    if (Date.now() > deadline) {
      throw new Error(`the agent did not write ${id} into ${file}. The file holds: ${text}`)
    }
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
}
