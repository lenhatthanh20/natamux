import { describe, expect, it } from 'vitest'
import { terminalClientMessageSchema, terminalSizeSchema } from './terminal.js'

describe('terminal messages', () => {
  it('reads a size from the query of the socket address', () => {
    expect(terminalSizeSchema.parse({ cols: '120', rows: '32' })).toEqual({ cols: 120, rows: 32 })
    expect(terminalSizeSchema.safeParse({ cols: '0', rows: '32' }).success).toBe(false)
    expect(terminalSizeSchema.safeParse({ cols: '1001', rows: '32' }).success).toBe(false)
  })

  it('takes input and a resize from the dashboard, and nothing else', () => {
    const accepts = (message: object) => terminalClientMessageSchema.safeParse(message).success
    expect(accepts({ type: 'input', data: 'ls\r' })).toBe(true)
    expect(accepts({ type: 'resize', cols: 80, rows: 24 })).toBe(true)
    expect(accepts({ type: 'exit', code: 0 })).toBe(false)
  })
})
