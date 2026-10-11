import { z } from 'zod'

// Messages on the WebSocket of a task terminal. Each frame is one JSON object.

const MAX_INPUT_CHARS = 64 * 1024
const MAX_SIZE = 1000

// How much history a terminal keeps above its screen: the dashboard, and the screen the daemon
// keeps for each shell, so a new dashboard can show all of it again. Like Ghostty, the limit is
// memory, not lines: a wide terminal keeps fewer rows than a narrow one.
export const TERMINAL_HISTORY_BYTES = 10_000_000
// What xterm.js holds for one cell, measured with 10,000 rows of 120 columns: 12 bytes of data
// and its share of the row object.
const BYTES_PER_CELL = 16
export const TERMINAL_SCROLLBACK_MIN_LINES = 1000
export const TERMINAL_SCROLLBACK_MAX_LINES = 10_000

// The rows a terminal of this width keeps above its screen.
export function scrollbackLines(cols: number): number {
  const fit = Math.floor(TERMINAL_HISTORY_BYTES / (Math.max(cols, 1) * BYTES_PER_CELL))
  return Math.min(TERMINAL_SCROLLBACK_MAX_LINES, Math.max(TERMINAL_SCROLLBACK_MIN_LINES, fit))
}

export const terminalSizeSchema = z.object({
  cols: z.coerce.number().int().min(1).max(MAX_SIZE),
  rows: z.coerce.number().int().min(1).max(MAX_SIZE),
})

export type TerminalSize = z.infer<typeof terminalSizeSchema>

// From the dashboard to the daemon.
export const terminalClientMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('input'), data: z.string().max(MAX_INPUT_CHARS) }),
  z.object({
    type: z.literal('resize'),
    cols: z.number().int().min(1).max(MAX_SIZE),
    rows: z.number().int().min(1).max(MAX_SIZE),
  }),
])

export type TerminalClientMessage = z.infer<typeof terminalClientMessageSchema>

// From the daemon to the dashboard. `exit` ends the session; `error` says why it could not start.
export const terminalServerMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('output'), data: z.string() }),
  z.object({ type: z.literal('exit'), code: z.number().int().nullable() }),
  z.object({ type: z.literal('error'), message: z.string() }),
])

export type TerminalServerMessage = z.infer<typeof terminalServerMessageSchema>
