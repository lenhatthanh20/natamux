import { describe, expect, it } from 'vitest'
import { TERMINAL_HISTORY_CHARS, TERMINAL_SCROLLBACK_LINES } from './terminal.js'

describe('terminal limits', () => {
  // The daemon counts chars and the dashboard counts lines. The rule: a line is 80 chars plus
  // escapes, so 200 chars a line is what the daemon must hold for every line the dashboard shows.
  it('lets the daemon keep at least as many lines as the dashboard can show', () => {
    expect(TERMINAL_HISTORY_CHARS).toBeGreaterThanOrEqual(TERMINAL_SCROLLBACK_LINES * 200)
  })
})
