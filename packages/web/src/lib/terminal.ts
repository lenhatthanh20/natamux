import {
  type TerminalServerMessage,
  type TerminalSize,
  terminalServerMessageSchema,
} from '@natamux/core'

// What a terminal tells the daemon when it connects. `visible` is false when the page is hidden:
// such a client does not take the shell's size (spec 2.1).
export type TerminalConnectQuery = TerminalSize & { visible: boolean }

export function terminalSocketUrl(
  location: Pick<Location, 'protocol' | 'host'>,
  taskId: string,
  ticket: string,
  query: TerminalConnectQuery,
  terminal = '1',
): string {
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws'
  const params = new URLSearchParams({
    ticket,
    cols: String(query.cols),
    rows: String(query.rows),
    terminal,
    visible: query.visible ? '1' : '0',
  })
  return `${scheme}://${location.host}/api/tasks/${encodeURIComponent(taskId)}/terminal?${params}`
}

// Returns undefined for a frame this dashboard does not understand.
export function parseTerminalMessage(text: string): TerminalServerMessage | undefined {
  try {
    const parsed = terminalServerMessageSchema.safeParse(JSON.parse(text))
    return parsed.success ? parsed.data : undefined
  } catch {
    return undefined
  }
}

// Answers xterm gives to queries in the output. Replayed output can hold old queries, and their
// answers must not reach the shell (spec 6). Written as source text, because biome refuses
// control characters in a regex literal.
const REPLY_FORMS = [
  String.raw`\x1b\[[?>][\d;]*c`, // DA1, DA2
  String.raw`\x1b\[\??\d+;\d+R`, // CPR, DECXCPR
  String.raw`\x1b\[\??[\d;]*n`, // DSR
  String.raw`\x1b\[\??[\d;]*\$y`, // DECRQM
  String.raw`\x1b\[[468];[\d;]*t`, // window size
  String.raw`\x1bP[\s\S]*?\x1b\\`, // DCS ... ST
  String.raw`\x1b\][\s\S]*?(?:\x1b\\|\x07)`, // OSC ... ST or BEL
]
const QUERY_REPLY = new RegExp(`^(?:${REPLY_FORMS.join('|')})+$`)

// True when the whole string is one or more query replies. Text that only contains one is not.
export function isQueryReply(data: string): boolean {
  return QUERY_REPLY.test(data)
}

// A terminal that opens takes the keyboard, unless the user is on the tab list or on the page
// itself: a number key that chose the tab must keep working for the next key.
export function takesFocusOnOpen(active: Element | null, body: Element): boolean {
  return active !== null && active !== body && active.getAttribute('role') !== 'tab'
}
