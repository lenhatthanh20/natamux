export type Theme = 'dark' | 'light'

const THEME_KEY = 'helmsman.theme'

// Dark unless this browser chose light. Storage can be missing or blocked.
export function savedTheme(): Theme {
  try {
    return globalThis.localStorage?.getItem(THEME_KEY) === 'light' ? 'light' : 'dark'
  } catch {
    return 'dark'
  }
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme
  try {
    globalThis.localStorage?.setItem(THEME_KEY, theme)
  } catch {
    // Not kept; the next visit starts dark.
  }
}
