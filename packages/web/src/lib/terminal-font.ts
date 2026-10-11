import { useSyncExternalStore } from 'react'
import { MONO_FONT } from './fonts'
import { TERMINAL_FONT_SIZE } from './terminal-theme'

export type TerminalFont = { family: string; size: number }
export type TerminalFontSource = 'custom' | 'ghostty' | 'default'

export const TERMINAL_FONT_MIN_SIZE = 8
export const TERMINAL_FONT_MAX_SIZE = 32

// The user's own choice.
const FONT_KEY = 'natamux.terminal-font'
// Ghostty's font, written by the desktop app from Ghostty's config (desktop/src/preload.cts).
export const GHOSTTY_FONT_KEY = 'natamux.ghostty-font'

const listeners = new Set<() => void>()

// A font from storage, or undefined when it is missing or not a font.
export function parseTerminalFont(
  raw: string | null | undefined,
): Partial<TerminalFont> | undefined {
  if (!raw) {
    return undefined
  }
  try {
    const value: unknown = JSON.parse(raw)
    if (typeof value !== 'object' || value === null) {
      return undefined
    }
    const { family, size } = value as Record<string, unknown>
    const font: Partial<TerminalFont> = {}
    if (typeof family === 'string' && family.trim() !== '') {
      font.family = family.trim()
    }
    if (
      typeof size === 'number' &&
      size >= TERMINAL_FONT_MIN_SIZE &&
      size <= TERMINAL_FONT_MAX_SIZE
    ) {
      font.size = size
    }
    return font.family === undefined && font.size === undefined ? undefined : font
  } catch {
    return undefined
  }
}

function read(key: string): Partial<TerminalFont> | undefined {
  try {
    return parseTerminalFont(globalThis.localStorage?.getItem(key))
  } catch {
    return undefined
  }
}

// The family as CSS: the chosen font first, the built-in ones after, for a font that is not
// installed and for the glyphs it lacks.
export function fontStack(family: string | undefined): string {
  const name = family?.replace(/["\\]/g, '').trim()
  return name ? `"${name}", ${MONO_FONT}` : MONO_FONT
}

export type ResolvedTerminalFont = {
  // The family as CSS, and the font the user named, if any.
  stack: string
  family: string | undefined
  size: number
  source: TerminalFontSource
  // What the user saved, before Ghostty's and the built-in font fill the rest.
  saved: Partial<TerminalFont> | undefined
}

// Each part from the user's font first, then Ghostty's, then the built-in one.
export function terminalFont(): ResolvedTerminalFont {
  const custom = read(FONT_KEY)
  const ghostty = read(GHOSTTY_FONT_KEY)
  const family = custom?.family ?? ghostty?.family
  return {
    stack: fontStack(family),
    family,
    size: custom?.size ?? ghostty?.size ?? TERMINAL_FONT_SIZE,
    source: custom ? 'custom' : ghostty ? 'ghostty' : 'default',
    saved: custom,
  }
}

// Ghostty's font, when the desktop app found one.
export function ghosttyFont(): Partial<TerminalFont> | undefined {
  return read(GHOSTTY_FONT_KEY)
}

// Saves the user's font, or goes back to Ghostty's or the built-in one with null. A part left out
// comes from Ghostty's or the built-in font.
export function saveTerminalFont(font: Partial<TerminalFont> | null): void {
  try {
    if (font === null) {
      globalThis.localStorage?.removeItem(FONT_KEY)
    } else {
      globalThis.localStorage?.setItem(FONT_KEY, JSON.stringify(font))
    }
  } catch {
    // Not kept; this page still uses it until it reloads.
  }
  for (const listener of listeners) {
    listener()
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

// A string, so the snapshot stays equal while nothing changes.
function snapshot(): string {
  const font = terminalFont()
  return `${font.source}|${font.size}|${font.stack}`
}

export function useTerminalFont(): ResolvedTerminalFont {
  useSyncExternalStore(subscribe, snapshot, snapshot)
  return terminalFont()
}
