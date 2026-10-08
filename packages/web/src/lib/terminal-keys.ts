import { isMac } from './keys'

export type TerminalShortcut = 'maximize' | 'find' | 'new-shell'

type KeyInfo = Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>

// The keys that the app keeps for itself inside a terminal. On a Mac, Cmd chords never reach the
// shell. Elsewhere Ctrl+letter belongs to the shell, so only Ctrl+Shift chords are taken.
export function terminalShortcut(event: KeyInfo, mac = isMac()): TerminalShortcut | null {
  if (event.altKey) {
    return null
  }
  const mod = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
  if (event.ctrlKey && event.shiftKey && !event.metaKey && event.code === 'Backquote') {
    return 'new-shell'
  }
  if (mod && event.shiftKey && event.key === 'Enter') {
    return 'maximize'
  }
  if (mod && event.key.toLowerCase() === 'f' && event.shiftKey !== mac) {
    return 'find'
  }
  return null
}

export function findShortcut(mac = isMac()): string {
  return mac ? 'Mod+F' : 'Ctrl+Shift+F'
}
