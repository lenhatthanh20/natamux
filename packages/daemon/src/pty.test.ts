import { afterEach, describe, expect, it, vi } from 'vitest'
import { spawnShell } from './pty.js'

// The native module is never loaded: the fake keeps the options and the data listener.
const native = vi.hoisted(() => ({
  options: [] as unknown[],
  emit: (_data: string | Buffer): void => undefined,
  signals: [] as (string | undefined)[],
  // What `ps -A -o pid=,ppid=,stat=` prints.
  table: '',
}))

vi.mock('node:child_process', () => ({
  execFile: vi.fn(),
  execFileSync: () => native.table,
}))

vi.mock('node-pty', () => ({
  spawn: (_file: string, _args: string[], options: unknown) => {
    native.options.push(options)
    return {
      pid: 10,
      onData: (listener: (data: string | Buffer) => void) => {
        native.emit = listener
      },
      kill: (signal?: string) => {
        native.signals.push(signal)
      },
    }
  },
}))

describe('spawnShell', () => {
  it('reads bytes from the pty, and turns the text Windows gives into bytes', () => {
    const shell = spawnShell({ cols: 80, rows: 24, cwd: '/work', env: {} })
    const heard: Buffer[] = []
    shell.onData((data) => heard.push(data))
    expect(native.options.at(-1)).toMatchObject({ cols: 80, rows: 24, encoding: null })

    // The first two of the three bytes of "ệ", as a read can stop inside a character.
    const cut = Buffer.from('ệ').subarray(0, 2)
    native.emit(cut)
    native.emit('Tiếng Việt 😀')

    expect(heard[0]).toBe(cut)
    expect(Buffer.isBuffer(heard[1])).toBe(true)
    expect(heard[1]?.toString()).toBe('Tiếng Việt 😀')
  })
})

describe.skipIf(process.platform === 'win32')('killing a shell', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('kills a program that ignored SIGHUP, also once the shell has ended', () => {
    const signalled: [number, string | number | undefined][] = []
    vi.spyOn(process, 'kill').mockImplementation((pid, signal) => {
      signalled.push([pid, signal])
      return true
    })
    const shell = spawnShell({ cols: 80, rows: 24, cwd: '/work', env: {} })
    expect(shell.leftover?.()).toBe(false)

    // 11 is an agent that will ignore SIGHUP, 12 its child, 13 a job that ends on SIGHUP.
    native.table = ['10 1 Ss', '11 10 S+', '12 11 S+', '13 10 S', ''].join('\n')
    shell.kill('SIGHUP')
    expect(signalled).toEqual([
      [13, 'SIGHUP'],
      [12, 'SIGHUP'],
      [11, 'SIGHUP'],
    ])
    expect(native.signals.at(-1)).toBe('SIGHUP')

    // The shell ended. The agent lives on under pid 1; the job is a zombie.
    native.table = ['11 1 S', '12 11 S', '13 1 Z', ''].join('\n')
    expect(shell.leftover?.()).toBe(true)

    signalled.length = 0
    shell.kill('SIGKILL')
    // Not to the ended job: its pid may belong to another process by now.
    expect(signalled).toEqual([
      [12, 'SIGKILL'],
      [11, 'SIGKILL'],
    ])
    expect(native.signals.at(-1)).toBe('SIGKILL')

    native.table = ''
    expect(shell.leftover?.()).toBe(false)
  })

  it('sends no SIGKILL to a process that is gone from ps', () => {
    const signalled: [number, string | number | undefined][] = []
    vi.spyOn(process, 'kill').mockImplementation((pid, signal) => {
      signalled.push([pid, signal])
      return true
    })
    const shell = spawnShell({ cols: 80, rows: 24, cwd: '/work', env: {} })
    native.table = ['10 1 Ss', '11 10 S+', '13 10 S', ''].join('\n')
    shell.kill('SIGHUP')

    native.table = ['11 1 S', ''].join('\n')
    signalled.length = 0
    shell.kill('SIGKILL')
    expect(signalled).toEqual([[11, 'SIGKILL']])
  })

  it('has no leftover when every process ended on SIGHUP', () => {
    vi.spyOn(process, 'kill').mockImplementation(() => true)
    const shell = spawnShell({ cols: 80, rows: 24, cwd: '/work', env: {} })
    native.table = ['10 1 Ss', '11 10 S+', ''].join('\n')
    shell.kill('SIGHUP')

    native.table = ['11 1 Z', ''].join('\n')
    expect(shell.leftover?.()).toBe(false)
  })
})
