import { execFile, execFileSync } from 'node:child_process'
import { promisify } from 'node:util'
import { spawn } from 'node-pty'
import { TtlCache } from '../shared/ttl-cache.js'
import { commandsUnder, descendants, stillRunning } from './process-tree.js'
import { shellCommand } from './shell-command.js'
import type { SpawnPty } from './terminal-types.js'

const execFileAsync = promisify(execFile)

// Each agent pane asks for the commands of its shell on a timer, so one `ps` serves them all for a
// moment.
const PROCESS_TABLE_MS = 2000
const processTable = new TtlCache<string>(PROCESS_TABLE_MS)

async function readProcessTable(): Promise<string> {
  const { stdout } = await execFileAsync('ps', ['-ww', '-A', '-o', 'pid=,ppid=,args='])
  return stdout
}

function processTableNow(): string {
  try {
    return execFileSync('ps', ['-A', '-o', 'pid=,ppid=,stat='], { encoding: 'utf8' })
  } catch {
    return ''
  }
}

// An agent runs as a job of the shell. Ending only the shell leaves it running with no terminal,
// so every process under the shell gets the signal too. So do the ones of `others` that still
// run, and what runs under them: processes an earlier signal reached, which may have outlived the
// shell. An ended one is skipped, as its pid may belong to another process by now. Gives the pids
// it signalled.
function signalTree(root: number, signal: string | undefined, others: number[] = []): number[] {
  const table = processTableNow()
  const alive = stillRunning(table, others)
  const pids = new Set([root, ...alive].flatMap((pid) => descendants(table, pid).reverse()))
  for (const pid of alive) {
    pids.add(pid)
  }
  for (const pid of pids) {
    try {
      process.kill(pid, (signal ?? 'SIGHUP') as NodeJS.Signals)
    } catch {
      // It ended already.
    }
  }
  return [...pids]
}

// The real pseudo terminal. Only main.ts imports this file, so tests never load the native module.
export const spawnShell: SpawnPty = ({ cols, rows, cwd, env }) => {
  const shell = shellCommand(env, process.platform)
  // Bytes, not text: the screen and the clients take UTF-8 as it is, so nothing decodes and
  // encodes again.
  const pty = spawn(shell.file, shell.args, {
    name: 'xterm-256color',
    cols,
    rows,
    cwd,
    env,
    encoding: null,
  })
  // The processes the SIGHUP of a close reached. One that ignored it may outlive the shell, so the
  // SIGKILL goes to it too. shortcut: a pid could be reused before the SIGKILL; pids count up, so
  // within KILL_GRACE_MS this is very unlikely.
  let hungUp: number[] = []
  return {
    onData: (listener) => {
      // Windows gives text whatever the encoding.
      pty.onData((data: string | Buffer) => {
        listener(typeof data === 'string' ? Buffer.from(data) : data)
      })
    },
    onExit: (listener) => {
      pty.onExit(({ exitCode }) => listener(exitCode))
    },
    write: (data) => pty.write(data),
    resize: (cols, rows) => pty.resize(cols, rows),
    pause: () => pty.pause(),
    resume: () => pty.resume(),
    // Windows has no signals; the shell and what runs in it are ended outright there.
    kill: (signal) => {
      if (process.platform !== 'win32') {
        if (signal === 'SIGKILL') {
          signalTree(pty.pid, signal, hungUp)
          hungUp = []
        } else {
          hungUp = signalTree(pty.pid, signal)
        }
      }
      pty.kill(process.platform === 'win32' ? undefined : signal)
    },
    leftover: () => hungUp.length > 0 && stillRunning(processTableNow(), hungUp).length > 0,
    foreground: () => pty.process,
    // Windows has no `ps`.
    commands: async () => {
      if (process.platform === 'win32') {
        return undefined
      }
      try {
        return commandsUnder(await processTable.get('', readProcessTable), pty.pid)
      } catch {
        return undefined
      }
    },
  }
}
