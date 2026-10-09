export function onMac(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)
}

// With Option held, a Mac types another character, so the physical key is checked too.
type ChordEvent = Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'> & {
  code?: string
}

// Mod is Cmd on macOS and Ctrl elsewhere.
export function isModChord(
  event: ChordEvent,
  key: string,
  { alt = false, mac = onMac() }: { alt?: boolean; mac?: boolean } = {},
): boolean {
  const mod = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
  const same = event.key.toLowerCase() === key || event.code === `Key${key.toUpperCase()}`
  return mod && event.altKey === alt && !event.shiftKey && same
}

// Inside a terminal on Windows and Linux, Ctrl+letter belongs to the shell. Only the Mac's Cmd
// chords reach the app from there.
export function chordReachesApp(inTerminal: boolean, mac = onMac()): boolean {
  return !inTerminal || mac
}
