import { describe, expect, it } from 'vitest'
import { TERMINAL_HISTORY_CHARS, TERMINAL_SCROLLBACK_LINES } from './terminal.js'

describe('terminal limits', () => {
  it('lets the daemon keep at least as many plain lines as the dashboard can show', () => {
    expect(TERMINAL_HISTORY_CHARS).toBeGreaterThanOrEqual(TERMINAL_SCROLLBACK_LINES * 80)
  })
})
