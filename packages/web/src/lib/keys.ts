export function isMac(): boolean {
  if (typeof navigator === 'undefined') {
    return false
  }
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.platform
  return /mac|iphone|ipad/i.test(platform ?? '')
}

// macOS shows glyphs in the system order (control, option, shift, command), joined with no sign.
const MAC_ORDER = ['Ctrl', 'Alt', 'Shift', 'Mod']
const MAC_GLYPH: Record<string, string> = {
  Mod: '⌘',
  Ctrl: '⌃',
  Alt: '⌥',
  Shift: '⇧',
  Enter: '↵',
  Backspace: '⌫',
  Tab: '⇥',
  Up: '↑',
  Down: '↓',
  Left: '←',
  Right: '→',
}

function keyLabel(key: string): string {
  return key.length === 1 ? key.toUpperCase() : key
}

// 'Mod+Shift+Enter' -> '⇧⌘↵' on macOS, 'Ctrl+Shift+Enter' elsewhere.
export function formatShortcut(shortcut: string, mac: boolean = isMac()): string {
  const keys = shortcut.split('+').filter((key) => key !== '')
  if (mac) {
    const modifiers = MAC_ORDER.filter((name) => keys.includes(name))
    const rest = keys.filter((key) => !MAC_ORDER.includes(key))
    return [...modifiers, ...rest].map((key) => MAC_GLYPH[key] ?? keyLabel(key)).join('')
  }
  return keys.map((key) => (key === 'Mod' ? 'Ctrl' : keyLabel(key))).join('+')
}
