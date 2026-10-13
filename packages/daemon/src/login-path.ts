// The desktop app reads the PATH of the user's login shell while the daemon starts: right after
// login the shell can take seconds. It sends the PATH once the daemon says it is ready. Until then
// the daemon has the app's own PATH, which may miss Homebrew, nvm and the like, so work that runs
// the user's programs waits for it.

// Set by the desktop app when it will send the PATH.
const PENDING = 'NATAMUX_PATH_PENDING'
// Longer than the app's 10 s limit for the shell. A PATH that does not come is a bug; the daemon
// then keeps the PATH it has, as the app does when the shell fails.
const WAIT_MS = 15_000

// The part of Electron's `process.parentPort` that the daemon uses.
export type ParentPort = {
  on(event: 'message', listener: (event: { data: unknown }) => void): unknown
  postMessage(message: unknown): void
}

// Sets the login PATH in `env` when it comes and resolves with it. Resolves at once with undefined
// when there is none to wait for, and with undefined when it does not come in time.
export function waitForLoginPath(
  env: NodeJS.ProcessEnv,
  port: ParentPort | undefined,
  waitMs = WAIT_MS,
): Promise<string | undefined> {
  const pending = env[PENDING] === '1'
  // The shells and agents must not see it.
  delete env[PENDING]
  if (!pending || port === undefined) {
    return Promise.resolve(undefined)
  }
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(undefined), waitMs)
    port.on('message', ({ data }) => {
      if (
        typeof data === 'object' &&
        data !== null &&
        'path' in data &&
        typeof data.path === 'string'
      ) {
        clearTimeout(timer)
        env.PATH = data.path
        resolve(data.path)
      }
    })
  })
}
