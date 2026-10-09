import type { PtyProcess, SpawnPty } from './terminal-service.js'

// A pseudo terminal for tests. It echoes what is written, and remembers how it was started.
let nextPid = 1000

export class FakePty implements PtyProcess {
  readonly pid = nextPid++
  readonly written: string[] = []
  readonly sizes: { cols: number; rows: number }[] = []
  killed = false
  readonly signals: (string | undefined)[] = []
  // A process that ignores everything but SIGKILL, like a program that traps SIGHUP.
  stubborn = false
  private dataListeners: ((data: string) => void)[] = []
  private exitListeners: ((code: number | null) => void)[] = []

  constructor(readonly options: Parameters<SpawnPty>[0]) {}

  onData(listener: (data: string) => void): void {
    this.dataListeners.push(listener)
  }

  onExit(listener: (code: number | null) => void): void {
    this.exitListeners.push(listener)
  }

  write(data: string): void {
    this.written.push(data)
    this.emit(data)
  }

  resize(cols: number, rows: number): void {
    this.sizes.push({ cols, rows })
  }

  kill(signal?: string): void {
    this.signals.push(signal)
    if (this.stubborn && signal !== 'SIGKILL') {
      return
    }
    this.killed = true
    this.exit(null)
  }

  emit(data: string): void {
    for (const listener of this.dataListeners) {
      listener(data)
    }
  }

  exit(code: number | null): void {
    for (const listener of this.exitListeners) {
      listener(code)
    }
  }
}

export function fakeSpawner(): { spawn: SpawnPty; ptys: FakePty[] } {
  const ptys: FakePty[] = []
  return {
    ptys,
    spawn: (options) => {
      const pty = new FakePty(options)
      ptys.push(pty)
      return pty
    },
  }
}
