import { execFile } from 'node:child_process'
import { createServer } from 'node:net'

// The preferred port when it is free, else any free port. The CLI looks at 4317 by default, but
// a dev daemon or another app may hold it.
export function freePort(preferred: number): Promise<number> {
  const tryPort = (port: number) =>
    new Promise<number>((resolve, reject) => {
      const server = createServer()
      server.once('error', reject)
      server.listen(port, '127.0.0.1', () => {
        const address = server.address()
        const taken = typeof address === 'object' && address !== null ? address.port : port
        server.close(() => resolve(taken))
      })
    })
  return tryPort(preferred).catch(() => tryPort(0))
}

// Waits until the daemon answers /health. Stops early when the daemon has exited.
export async function waitForHealth(
  url: string,
  options: { timeoutMs: number; exited: () => boolean; fetch?: typeof fetch; stepMs?: number },
): Promise<void> {
  const get = options.fetch ?? fetch
  const until = Date.now() + options.timeoutMs
  while (Date.now() < until) {
    if (options.exited()) {
      throw new Error('The daemon stopped while it started')
    }
    try {
      const response = await get(url)
      if (response.ok) {
        return
      }
    } catch {
      // Not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, options.stepMs ?? 200))
  }
  throw new Error(`The daemon did not answer at ${url} in ${options.timeoutMs / 1000} seconds`)
}

const PATH_MARK = '__NATAMUX_PATH__'
// It runs once at app start. Heavy startup files on a cold disk right after login can pass 5 s.
const LOGIN_SHELL_TIMEOUT_MS = 10_000

export type LoginShellPath = { path: string } | { problem: string }

// The PATH that the user's login shell sets up, or why it could not be read.
export function loginShellPath(
  shell: string | undefined,
  run: typeof execFile = execFile,
): Promise<LoginShellPath> {
  if (shell === undefined || shell.trim() === '') {
    return Promise.resolve({ problem: '$SHELL is not set' })
  }
  return new Promise((resolve) => {
    // The marks keep what the shell's startup files print out of the answer.
    run(
      shell,
      ['-ilc', `printf '${PATH_MARK}%s${PATH_MARK}' "$PATH"`],
      { timeout: LOGIN_SHELL_TIMEOUT_MS, encoding: 'utf8' },
      (error, stdout, stderr) => {
        const match = String(stdout ?? '').match(new RegExp(`${PATH_MARK}(.*?)${PATH_MARK}`, 's'))
        const path = match?.[1]?.trim()
        // A failing line in a startup file makes the shell exit with an error after it printed
        // a good PATH.
        if (path) {
          resolve({ path })
          return
        }
        resolve({ problem: loginShellProblem(error, String(stderr ?? '')) })
      },
    )
  })
}

function loginShellProblem(
  error: (Error & { killed?: boolean; code?: unknown }) | null,
  stderr: string,
): string {
  if (error === null) {
    return 'it printed no PATH'
  }
  if (error.killed) {
    return `it took longer than ${LOGIN_SHELL_TIMEOUT_MS / 1000} s`
  }
  if (typeof error.code !== 'number') {
    return error.message
  }
  const firstLine = stderr
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line !== '')
  return `it exited with code ${error.code}${firstLine === undefined ? '' : `: ${firstLine}`}`
}

// The daemon log line for a login PATH that could not be read. None when it was read.
export function loginPathLogLine(
  login: LoginShellPath,
  fallback: string | undefined,
): string | undefined {
  if ('path' in login) {
    return undefined
  }
  return `Could not read the login shell PATH (${login.problem}); using ${fallback ?? '(none)'}\n`
}
