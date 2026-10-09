import {
  type TerminalServerMessage,
  type TerminalSize,
  terminalServerMessageSchema,
} from '@helmsman/core'

export function terminalSocketUrl(
  location: Pick<Location, 'protocol' | 'host'>,
  taskId: string,
  ticket: string,
  size: TerminalSize,
  terminal = '1',
): string {
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws'
  const query = new URLSearchParams({
    ticket,
    cols: String(size.cols),
    rows: String(size.rows),
    terminal,
  })
  return `${scheme}://${location.host}/api/tasks/${encodeURIComponent(taskId)}/terminal?${query}`
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

// A terminal that opens takes the keyboard, unless the user is on the tab list or on the page
// itself: a number key that chose the tab must keep working for the next key.
export function takesFocusOnOpen(active: Element | null, body: Element): boolean {
  return active !== null && active !== body && active.getAttribute('role') !== 'tab'
}
