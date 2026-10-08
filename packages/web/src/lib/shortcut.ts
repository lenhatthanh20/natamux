// Shortcuts are written as 'Mod+K', 'Mod+Shift+Enter', 'F1', '?' or 'Ctrl+Shift+`'.
// Mod is Cmd on a Mac and Ctrl elsewhere. Ctrl is always Ctrl.

export type ParsedShortcut = {
  mod: boolean
  ctrl: boolean
  alt: boolean
  shift: boolean
  key: string
}

export type ShortcutEvent = Pick<
  KeyboardEvent,
  'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'
> & { code?: string }

export function parseShortcut(shortcut: string): ParsedShortcut {
  const parts = shortcut.split('+')
  // A shortcut that ends in a plus sign has an empty last part.
  const key = (parts.pop() || '+').toLowerCase()
  const names = new Set(parts)
  return {
    mod: names.has('Mod'),
    ctrl: names.has('Ctrl'),
    alt: names.has('Alt'),
    shift: names.has('Shift'),
    key,
  }
}

function isPunctuation(key: string): boolean {
  return key.length === 1 && !/[a-z0-9]/.test(key)
}

function sameKey(event: ShortcutEvent, key: string): boolean {
  if (event.key.toLowerCase() === key) {
    return true
  }
  // With Option or Shift held, the typed character is another one, so the physical key counts.
  if (/^[a-z]$/.test(key)) {
    return event.code === `Key${key.toUpperCase()}`
  }
  if (/^[0-9]$/.test(key)) {
    return event.code === `Digit${key}`
  }
  return key === '`' && event.code === 'Backquote'
}

export function matchesShortcut(event: ShortcutEvent, shortcut: string, mac: boolean): boolean {
  const wanted = parseShortcut(shortcut)
  const modDown = mac ? event.metaKey : event.ctrlKey
  const modOther = mac ? event.ctrlKey : event.metaKey
  // Ctrl is the literal key; Mod may also be Cmd. The other one of the pair must stay up.
  const ctrlWanted = wanted.ctrl || (wanted.mod && !mac)
  const metaWanted = wanted.mod && mac
  if (event.ctrlKey !== ctrlWanted || event.metaKey !== metaWanted) {
    return false
  }
  if (wanted.mod && (!modDown || modOther)) {
    return false
  }
  if (event.altKey !== wanted.alt) {
    return false
  }
  // Shift decides the character of '?' and '[', so it only counts when the shortcut names it.
  if (wanted.shift) {
    if (!event.shiftKey) {
      return false
    }
  } else if (event.shiftKey && !isPunctuation(wanted.key)) {
    return false
  }
  return sameKey(event, wanted.key)
}

export function isFunctionKey(shortcut: string): boolean {
  return /^F\d{1,2}$/i.test(parseShortcut(shortcut).key.toUpperCase())
}

// A key with no modifier but Shift: it types a character, so a field or a shell keeps it.
export function isSingleKey(shortcut: string): boolean {
  const { mod, ctrl, alt, key } = parseShortcut(shortcut)
  return !mod && !ctrl && !alt && !isFunctionKey(shortcut) && key !== ''
}

// Inside a terminal the shell owns the keys. On a Mac Cmd chords never reach the shell, so they
// are the app's. Elsewhere only function keys and Ctrl+Shift chords are (Ctrl+K, Ctrl+B and
// Ctrl+J are the shell's own).
export function reachesAppFromTerminal(shortcut: string, mac: boolean): boolean {
  if (isFunctionKey(shortcut)) {
    return true
  }
  const { mod, ctrl, shift } = parseShortcut(shortcut)
  if (mac && mod) {
    return true
  }
  return (mod || ctrl) && shift
}
