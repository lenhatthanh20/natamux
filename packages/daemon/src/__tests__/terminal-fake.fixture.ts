import type { PtyProcess, SpawnPty } from '../terminal-service.js'

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
  // A program under the shell that ignores SIGHUP: the shell ends, the program lives on until
  // SIGKILL.
  orphan = false
  // The program in the foreground; tests set it to pretend an agent runs.
  program = 'bash'
  // The command lines of the processes under the shell; undefined where the platform cannot list
  // them.
  commandLines: string[] | undefined = undefined
  // pause() and resume() calls, in order. emit() still delivers output while paused.
  readonly flow: ('pause' | 'resume')[] = []
  paused = false
  private dataListeners: ((data: Buffer) => void)[] = []
  private exitListeners: ((code: number | null) => void)[] = []

  constructor(readonly options: Parameters<SpawnPty>[0]) {}

  onData(listener: (data: Buffer) => void): void {
    this.dataListeners.push(listener)
  }

  onExit(listener: (code: number | null) => void): void {
    this.exitListeners.push(listener)
  }

  foreground(): string {
    return this.program
  }

  async commands(): Promise<string[] | undefined> {
    return this.commandLines
  }

  write(data: string): void {
    this.written.push(data)
    this.emit(data)
  }

  resize(cols: number, rows: number): void {
    this.sizes.push({ cols, rows })
  }

  pause(): void {
    this.paused = true
    this.flow.push('pause')
  }

  resume(): void {
    this.paused = false
    this.flow.push('resume')
  }

  leftover(): boolean {
    return this.orphan
  }

  kill(signal?: string): void {
    this.signals.push(signal)
    if (signal === 'SIGKILL') {
      this.orphan = false
    }
    // A real shell reports its end once.
    if (this.killed) {
      return
    }
    if (this.stubborn && signal !== 'SIGKILL') {
      return
    }
    this.killed = true
    this.exit(null)
  }

  // Bytes, as the real pty gives them. Text is encoded as UTF-8.
  emit(data: string | Uint8Array): void {
    const bytes = Buffer.from(data)
    for (const listener of this.dataListeners) {
      listener(bytes)
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
