import { describe, expect, it, vi } from 'vitest'
import { spawnShell } from './pty.js'

// The native module is never loaded: the fake keeps the options and the data listener.
const native = vi.hoisted(() => ({
  options: [] as unknown[],
  emit: (_data: string | Buffer): void => undefined,
}))

vi.mock('node-pty', () => ({
  spawn: (_file: string, _args: string[], options: unknown) => {
    native.options.push(options)
    return {
      onData: (listener: (data: string | Buffer) => void) => {
        native.emit = listener
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
