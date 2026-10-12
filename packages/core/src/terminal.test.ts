import { describe, expect, it } from 'vitest'
import {
  scrollbackLines,
  TERMINAL_HISTORY_BYTES,
  TERMINAL_SCROLLBACK_MAX_LINES,
  TERMINAL_SCROLLBACK_MIN_LINES,
  terminalClientMessageSchema,
  terminalServerMessageSchema,
  terminalSizeSchema,
} from './terminal.js'

describe('scrollbackLines', () => {
  it('keeps about the same memory at any width, so a wide terminal keeps fewer rows', () => {
    expect(scrollbackLines(80)).toBe(7812)
    expect(scrollbackLines(120)).toBe(5208)
    expect(scrollbackLines(200)).toBe(3125)
    for (const cols of [80, 120, 200, 300]) {
      expect(scrollbackLines(cols) * cols * 16).toBeLessThanOrEqual(TERMINAL_HISTORY_BYTES)
    }
  })

  it('keeps at most the maximum on a narrow terminal, and at least the minimum on a huge one', () => {
    expect(scrollbackLines(20)).toBe(TERMINAL_SCROLLBACK_MAX_LINES)
    expect(scrollbackLines(1000)).toBe(TERMINAL_SCROLLBACK_MIN_LINES)
  })
})

describe('terminal messages', () => {
  it('reads a size from the query of the socket address', () => {
    expect(terminalSizeSchema.parse({ cols: '120', rows: '32' })).toEqual({ cols: 120, rows: 32 })
    expect(terminalSizeSchema.safeParse({ cols: '0', rows: '32' }).success).toBe(false)
    expect(terminalSizeSchema.safeParse({ cols: '1001', rows: '32' }).success).toBe(false)
  })

  it('takes input, a resize, visibility, acks and pings from the dashboard, and nothing else', () => {
    const accepts = (message: object) => terminalClientMessageSchema.safeParse(message).success
    expect(accepts({ type: 'input', data: 'ls\r' })).toBe(true)
    expect(accepts({ type: 'resize', cols: 80, rows: 24 })).toBe(true)
    expect(accepts({ type: 'visible', visible: false })).toBe(true)
    expect(accepts({ type: 'visible', visible: true, cols: 120, rows: 40 })).toBe(true)
    expect(accepts({ type: 'ack', offset: 0 })).toBe(true)
    expect(accepts({ type: 'ack', offset: 65_536 })).toBe(true)
    expect(accepts({ type: 'ping' })).toBe(true)
    expect(accepts({ type: 'visible', visible: true, cols: 0, rows: 40 })).toBe(false)
    expect(accepts({ type: 'ack', offset: -1 })).toBe(false)
    expect(accepts({ type: 'output', data: 'a' })).toBe(false)
    expect(accepts({ type: 'exit', code: 0 })).toBe(false)
    expect(accepts({ type: 'pong' })).toBe(false)
  })

  // Output goes in binary frames, so no JSON message carries it.
  it('sends control messages to the dashboard, and no output message', () => {
    const accepts = (message: object) => terminalServerMessageSchema.safeParse(message).success
    expect(accepts({ type: 'hello', shellId: 'abc' })).toBe(true)
    expect(accepts({ type: 'snapshot', cols: 80, rows: 24 })).toBe(true)
    expect(accepts({ type: 'resume', offset: 10 })).toBe(true)
    expect(accepts({ type: 'replayed', offset: 0 })).toBe(true)
    expect(accepts({ type: 'pong' })).toBe(true)
    expect(accepts({ type: 'exit', code: null })).toBe(true)
    expect(accepts({ type: 'error', message: 'No workspace' })).toBe(true)
    expect(accepts({ type: 'hello', shellId: '' })).toBe(false)
    expect(accepts({ type: 'replayed', offset: -1 })).toBe(false)
    expect(accepts({ type: 'output', data: 'a' })).toBe(false)
    expect(accepts({ type: 'ping' })).toBe(false)
  })
})
