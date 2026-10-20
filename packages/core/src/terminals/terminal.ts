import { z } from 'zod'

// Messages on the WebSocket of a task terminal. Output goes in binary frames: the raw UTF-8 bytes
// for xterm. Every other message is one JSON object in a text frame.

const MAX_INPUT_CHARS = 64 * 1024
const MAX_SIZE = 1000

// The client acks the output it has parsed after this many bytes, and once after a catch-up.
export const ACK_EVERY_BYTES = 64 * 1024

// Close codes of the terminal socket, in the range that applications may use. After any of them
// the dashboard does not reconnect. GONE: the shell of a resume no longer runs.
export const CLOSE_UNAUTHORIZED = 4401
export const CLOSE_REFUSED = 4409
export const CLOSE_GONE = 4410

// The dashboard pings this often while the page is visible, and reconnects when no pong came for
// two of these. The daemon closes a socket that sent nothing for SILENT_CLOSE_MS: a page in the
// background throttles its timers, and reconnects by itself when it comes back.
export const HEARTBEAT_MS = 15_000
export const SILENT_CLOSE_MS = 90_000

// A lost socket is tried again after min(RECONNECT_MAX_MS, RECONNECT_BASE_MS * 2 ** try), give or
// take RECONNECT_JITTER of it, so many pages do not come back at the same moment.
export const RECONNECT_BASE_MS = 500
export const RECONNECT_MAX_MS = 10_000
export const RECONNECT_JITTER = 0.2

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

// OSC (links, titles), CSI (colors, cursor moves, the end of a paste) and two-byte sequences.
// biome-ignore lint/suspicious/noControlCharactersInRegex: escape sequences start with ESC
const ESCAPES = /\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~]|\x1b[@-Z\\-_]/g
// biome-ignore lint/suspicious/noControlCharactersInRegex: these are the bytes it removes
const CONTROLS = /[\x00-\x08\x0b-\x1f\x7f-\x9f\ufeff]/g

// Text with no escape sequences or control bytes except tabs and line breaks, and with LF line
// ends. Pasted into an agent, an escape can end the paste early, so its Enter is not a submit.
export function plainText(text: string): string {
  return text.replace(ESCAPES, '').replace(/\r\n?/g, '\n').replace(CONTROLS, '')
}

export const terminalSizeSchema = z.object({
  cols: z.coerce.number().int().min(1).max(MAX_SIZE),
  rows: z.coerce.number().int().min(1).max(MAX_SIZE),
})

export type TerminalSize = z.infer<typeof terminalSizeSchema>

const sizeSchema = z.number().int().min(1).max(MAX_SIZE)
// Bytes of output since the shell started.
const offsetSchema = z.number().int().min(0)
// Random, set when a shell starts. A socket with `shellId` in its query resumes that shell.
export const shellIdSchema = z.string().min(1).max(64)

// From the dashboard to the daemon.
export const terminalClientMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('input'), data: z.string().max(MAX_INPUT_CHARS) }),
  z.object({ type: z.literal('resize'), cols: sizeSchema, rows: sizeSchema }),
  // cols and rows come only when the terminal itself came on screen; only then it claims the size.
  z.object({
    type: z.literal('visible'),
    visible: z.boolean(),
    cols: sizeSchema.optional(),
    rows: sizeSchema.optional(),
  }),
  z.object({ type: z.literal('ack'), offset: offsetSchema }),
  z.object({ type: z.literal('ping') }),
])

export type TerminalClientMessage = z.infer<typeof terminalClientMessageSchema>

// From the daemon to the dashboard. `hello` is always first. `snapshot` and `resume` start a
// catch-up and `replayed` ends it. `pong` answers `ping`. `exit` ends the session; `error` says why
// it could not start.
export const terminalServerMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('hello'), shellId: shellIdSchema }),
  z.object({ type: z.literal('snapshot'), cols: sizeSchema, rows: sizeSchema }),
  z.object({ type: z.literal('resume'), offset: offsetSchema }),
  z.object({ type: z.literal('replayed'), offset: offsetSchema }),
  z.object({ type: z.literal('pong') }),
  z.object({ type: z.literal('exit'), code: z.number().int().nullable() }),
  z.object({ type: z.literal('error'), message: z.string() }),
])

export type TerminalServerMessage = z.infer<typeof terminalServerMessageSchema>

// The last screen of a shell, kept as plain text when its task was done or cancelled, so the
// wrap-up can show it after the shell is gone. `agent` is null for a plain shell, `stage` for a
// shell that no stage used (and in files from before stages were kept). `session` is the agent's
// last session, so the agent can be reopened on it; null when none is known.
export const ENDED_SHELL_LINES = 200
export const ENDED_SHELL_MAX_CHARS = 64 * 1024

export const endedShellSchema = z.object({
  name: z.string().min(1).max(16),
  agent: z.string().min(1).nullable(),
  stage: z.string().min(1).nullable().default(null),
  session: z.string().min(1).nullable().default(null),
  endedAt: z.string().min(1),
  text: z.string().max(ENDED_SHELL_MAX_CHARS),
})

export type EndedShell = z.infer<typeof endedShellSchema>

// The names of shells. Another program in the foreground of a terminal means something runs in it.
export const SHELL_PROGRAMS: readonly string[] = [
  'bash',
  'zsh',
  'sh',
  'dash',
  'fish',
  'ksh',
  'nu',
  'pwsh',
  'powershell.exe',
  'cmd.exe',
]
