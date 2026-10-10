import { z } from 'zod'

// Messages on the WebSocket of a task terminal. Each frame is one JSON object.

const MAX_INPUT_CHARS = 64 * 1024
const MAX_SIZE = 1000

// How much history a terminal keeps: the dashboard keeps this many lines above the screen, and
// the daemon keeps this much output, so a new dashboard can show all of it again. Change them
// together.
export const TERMINAL_SCROLLBACK_LINES = 10_000
export const TERMINAL_HISTORY_CHARS = 2_000_000

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
