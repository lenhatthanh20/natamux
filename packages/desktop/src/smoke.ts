import { getHashes } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { verifySums } from './release-signature.js'

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

// Made by the real minisign tool with a test key; the same files are in src/fixtures/minisign.
// The smoke check runs it in the packaged app, so CI proves that Electron's crypto (BoringSSL, not
// OpenSSL) checks release signatures on every system.
export const SIGNATURE_FIXTURE = {
  publicKey: 'RWRlDDYUMCwx+Oo1OxIoyy7UaN2jSLqaEfEX9pmWdYLOJWE1WCES0Jkf',
  tag: 'v1.2.3',
  sums: [
    '85158a9de88b51d7eef46fd691356e8c838ce691939a458bd4f4bac5e36cd2f2  Natamux-linux-amd64.deb',
    '33ed21af8680f0b16480bb972e8a9a1231ddc80e65fa717ccd86e177d468a80f  Natamux-linux-x64.tar.gz',
    'c44f3b7d10d8dc7cc09bbc7bb66d6a15a13375241849fb9aa144bc7ba37d2cd3  Natamux-linux-x86_64.AppImage',
    'a3a807cf10fc02efe46f50cd4fd0eaa7a457d787c9d8a1bbfac1fa036e26cc36  Natamux-mac-arm64.dmg',
    '170785bba691a0e8dfd5c28229fc5258a10741d574fd21dbe7829cee8fb715eb  Natamux-mac-x64.dmg',
    '1e6e7727b0190c0bb0bcc8a7460595cf535a3d6bf886c2c3250555b271aeff85  Natamux-win-x64.exe',
    '',
  ].join('\n'),
  signatures: {
    ED: [
      'untrusted comment: signature from minisign secret key',
      'RURlDDYUMCwx+MAUAJZ5sAblZq9gM/EmLvmjiROJJor5wEZ50WnvBMFjkJR8Y9cASPNMFt1eEVmI+s6mrMjnbvApqjURr5O7FgU=',
      'trusted comment: natamux v1.2.3',
      '90jhZoxm5mHTijoh55DCPk4k1vabl3wzZBoaygbHsJedx4oo367SSVMiNIwpi0wqk+MQpuXa8/OsFiNB1oVZAQ==',
      '',
    ].join('\n'),
    Ed: [
      'untrusted comment: signature from minisign secret key',
      'RWRlDDYUMCwx+LaEHd99fDk59rGxpBnK7QgvowzUsviMSX0/tO7lwaEccTYdHYsk68fqJOgyU6LNp9MhTsC5lCJLt2Fm93WhiwU=',
      'trusted comment: natamux v1.2.3',
      'RtSEQ8Umu5X2lgBppgcPsGTBZARBP+teDQG7LJGCCNg7rxpgQe05kRXlz2QMCQl+50JHQ0wooNeR1IUlvCmsCA==',
      '',
    ].join('\n'),
  },
}

// Throws what failed. Returns a line for the log.
export function checkReleaseSignature(): string {
  const { publicKey, tag, sums, signatures } = SIGNATURE_FIXTURE
  const failures: string[] = []
  for (const [algorithm, signature] of Object.entries(signatures)) {
    const input = { publicKey, tag, signature, current: '0.0.0' }
    try {
      verifySums({ ...input, sums: Buffer.from(sums) })
    } catch (error) {
      failures.push(`${algorithm}: ${error instanceof Error ? error.message : String(error)}`)
    }
    try {
      verifySums({ ...input, sums: Buffer.from(sums.replace('85158a9d', '85158a9e')) })
      failures.push(`${algorithm}: a changed SHA256SUMS.txt passed`)
    } catch {
      // Expected.
    }
  }
  const blake2b = `blake2b512 ${getHashes().includes('blake2b512') ? 'is' : 'is not'} in getHashes()`
  if (failures.length > 0) {
    throw new Error(`the release signature check failed (${blake2b}): ${failures.join('; ')}`)
  }
  return `release signatures ED and Ed check out (${blake2b})`
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
