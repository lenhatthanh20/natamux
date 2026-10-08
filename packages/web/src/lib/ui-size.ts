import { useSyncExternalStore } from 'react'

export type UiSize = 'compact' | 'default' | 'large'

export const UI_SIZES: readonly UiSize[] = ['compact', 'default', 'large']
export const UI_SIZE_LABEL: Record<UiSize, string> = {
  compact: 'Compact',
  default: 'Default',
  large: 'Large',
}

// The body text is 12, 13 or 14px. Every size is in rem, so the whole UI follows.
const ZOOM: Record<UiSize, number> = { compact: 0.92, default: 1, large: 1.077 }

const UI_SIZE_KEY = 'helmsman.ui-size'
const listeners = new Set<() => void>()

export function uiSizeZoom(size: UiSize): number {
  return ZOOM[size]
}

export function parseUiSize(raw: string | null | undefined): UiSize {
  return raw === 'compact' || raw === 'large' ? raw : 'default'
}

export function savedUiSize(): UiSize {
  try {
    return parseUiSize(globalThis.localStorage?.getItem(UI_SIZE_KEY))
  } catch {
    return 'default'
  }
}

export function applyUiSize(size: UiSize): void {
  document.documentElement.style.setProperty('--ui-zoom', String(ZOOM[size]))
  try {
    globalThis.localStorage?.setItem(UI_SIZE_KEY, size)
  } catch {
    // Not kept; the next visit starts at the default size.
  }
  for (const listener of listeners) {
    listener()
  }
}

// Call once at start.
export function startUiSize(): void {
  document.documentElement.style.setProperty('--ui-zoom', String(ZOOM[savedUiSize()]))
}

function subscribe(onChange: () => void): () => void {
  listeners.add(onChange)
  return () => listeners.delete(onChange)
}

// The chosen size, for the menu's check mark and for xterm and Monaco, which set font sizes in px.
export function useUiSize(): UiSize {
  return useSyncExternalStore(subscribe, savedUiSize, () => 'default')
}
