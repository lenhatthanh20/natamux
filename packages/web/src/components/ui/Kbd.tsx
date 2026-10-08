import { formatShortcut } from '../../lib/keys'
import { kbd } from './styles'

// Names that read better as a symbol or a short word on a key cap.
const KEY_NAME: Record<string, string> = {
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Escape: 'Esc',
  ContextMenu: 'Menu',
}

function keyCaps(shortcut: string): string {
  return shortcut
    .split('+')
    .map((part) => KEY_NAME[part] ?? part)
    .join('+')
}

type KbdProps = {
  // For example 'Mod+K'. Shown as the keys of this platform.
  shortcut: string
  // On a filled button the chip has no box of its own.
  onInverted?: boolean
  className?: string
}

export function Kbd({ shortcut, onInverted = false, className = '' }: KbdProps) {
  return (
    <kbd
      className={`${onInverted ? 'inline-flex h-[1.125rem] items-center px-1 text-2xs font-medium opacity-70' : kbd} ${className}`}
    >
      {formatShortcut(keyCaps(shortcut))}
    </kbd>
  )
}
