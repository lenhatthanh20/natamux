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

// The PATH that the user's login shell sets up. Undefined when it cannot be read in time.
export function loginShellPath(
  shell: string | undefined,
  run: typeof execFile = execFile,
): Promise<string | undefined> {
  if (shell === undefined || shell === '') {
    return Promise.resolve(undefined)
  }
  return new Promise((resolve) => {
    // The marks keep what the shell's startup files print out of the answer.
    run(
      shell,
      ['-ilc', `printf '${PATH_MARK}%s${PATH_MARK}' "$PATH"`],
      { timeout: 5000, encoding: 'utf8' },
      (error, stdout) => {
        const match = String(stdout ?? '').match(new RegExp(`${PATH_MARK}(.*?)${PATH_MARK}`, 's'))
        resolve(error === null && match?.[1] ? match[1] : undefined)
      },
    )
  })
}
