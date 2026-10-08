import { existsSync } from 'node:fs'
import { spawn } from 'node-pty'
import type { SpawnPty } from './terminal-service.js'

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
    kill: () => pty.kill(),
  }
}
