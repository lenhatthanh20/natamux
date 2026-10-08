import { useSyncExternalStore } from 'react'

export type Theme = 'system' | 'light' | 'dark'
export type ResolvedTheme = 'light' | 'dark'

const THEME_KEY = 'helmsman.theme'

export function parseTheme(raw: string | null | undefined): Theme {
  return raw === 'light' || raw === 'dark' ? raw : 'system'
}

// The system's scheme counts only when the user left the choice on "system".
export function resolveTheme(theme: Theme, systemIsDark: boolean): ResolvedTheme {
  if (theme === 'system') {
    return systemIsDark ? 'dark' : 'light'
  }
  return theme
}

function systemQuery(): MediaQueryList | null {
  return typeof globalThis.matchMedia === 'function'
    ? globalThis.matchMedia('(prefers-color-scheme: dark)')
    : null
}

// First visit is "system". Storage can be missing or blocked.
export function savedTheme(): Theme {
  try {
    return parseTheme(globalThis.localStorage?.getItem(THEME_KEY))
  } catch {
    return 'system'
  }
}

export function resolvedTheme(theme: Theme = savedTheme()): ResolvedTheme {
  return resolveTheme(theme, systemQuery()?.matches ?? false)
}

// CSS reads the resolved value, so data-theme is always "light" or "dark".
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = resolvedTheme(theme)
  try {
    globalThis.localStorage?.setItem(THEME_KEY, theme)
  } catch {
    // Not kept; the next visit follows the system.
  }
}

// Call once at start: a system change moves the page only while the choice is "system".
export function startTheme(): void {
  document.documentElement.dataset.theme = resolvedTheme()
  systemQuery()?.addEventListener('change', () => {
    if (savedTheme() === 'system') {
      document.documentElement.dataset.theme = resolvedTheme('system')
    }
  })
}

function subscribe(onChange: () => void): () => void {
  const observer = new MutationObserver(onChange)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
  return () => observer.disconnect()
}

function snapshot(): ResolvedTheme {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light'
}

// The theme the page shows now, for things that cannot read CSS variables (Monaco, xterm).
export function useResolvedTheme(): ResolvedTheme {
  return useSyncExternalStore(subscribe, snapshot, () => 'dark')
}

// The UI size factor on <html>, for fonts that are set in px (xterm, Monaco).
export function uiZoom(): number {
  const value = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue('--ui-zoom'),
  )
  return Number.isFinite(value) && value > 0 ? value : 1
}
