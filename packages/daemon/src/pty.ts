import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { spawn } from 'node-pty'
import { descendants } from './process-tree.js'
import type { SpawnPty } from './terminal-service.js'

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

function shellPath(env: NodeJS.ProcessEnv): string {
  if (process.platform === 'win32') {
    return 'powershell.exe'
  }
  if (env.SHELL && existsSync(env.SHELL)) {
    return env.SHELL
  }
  return existsSync('/bin/bash') ? '/bin/bash' : '/bin/sh'
}

// The real pseudo terminal. Only main.ts imports this file, so tests never load the native module.
export const spawnShell: SpawnPty = ({ cols, rows, cwd, env }) => {
  const pty = spawn(shellPath(env), [], { name: 'xterm-256color', cols, rows, cwd, env })
  return {
    onData: (listener) => {
      pty.onData(listener)
    },
    onExit: (listener) => {
      pty.onExit(({ exitCode }) => listener(exitCode))
    },
    write: (data) => pty.write(data),
    resize: (cols, rows) => pty.resize(cols, rows),
    // Windows has no signals; the shell and what runs in it are ended outright there.
    kill: (signal) => {
      if (process.platform !== 'win32') {
        signalTree(pty.pid, signal)
      }
      pty.kill(process.platform === 'win32' ? undefined : signal)
    },
    foreground: () => pty.process,
  }
}
