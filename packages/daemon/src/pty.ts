import { execFile, execFileSync } from 'node:child_process'
import { promisify } from 'node:util'
import { spawn } from 'node-pty'
import { commandsUnder, descendants } from './process-tree.js'
import { shellCommand } from './shell-command.js'
import type { SpawnPty } from './terminal-service.js'

const execFileAsync = promisify(execFile)

// An agent runs as a job of the shell. Ending only the shell leaves it running with no terminal,
// so every process under the shell gets the signal too.
function signalTree(root: number, signal: string | undefined): void {
  let table = ''
  try {
    table = execFileSync('ps', ['-A', '-o', 'pid=,ppid='], { encoding: 'utf8' })
  } catch {
    return
  }
  for (const pid of descendants(table, root).reverse()) {
    try {
      process.kill(pid, (signal ?? 'SIGHUP') as NodeJS.Signals)
    } catch {
      // It ended already.
    }
  }
}

// The real pseudo terminal. Only main.ts imports this file, so tests never load the native module.
export const spawnShell: SpawnPty = ({ cols, rows, cwd, env }) => {
  const shell = shellCommand(env, process.platform)
  const pty = spawn(shell.file, shell.args, { name: 'xterm-256color', cols, rows, cwd, env })
  return {
    onData: (listener) => {
      pty.onData(listener)
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
        signalTree(pty.pid, signal)
      }
      pty.kill(process.platform === 'win32' ? undefined : signal)
    },
    foreground: () => pty.process,
    // Windows has no `ps`.
    commands: async () => {
      if (process.platform === 'win32') {
        return undefined
      }
      try {
        const { stdout } = await execFileAsync('ps', ['-ww', '-A', '-o', 'pid=,ppid=,args='])
        return commandsUnder(stdout, pty.pid)
      } catch {
        return undefined
      }
    },
  }
}
