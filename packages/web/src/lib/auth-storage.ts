const STORAGE_KEY = 'natamux.auth'

export function loadToken(): string | null {
  try {
    return window.localStorage.getItem(STORAGE_KEY)
  } catch {
    return null
  }
}

export function saveToken(token: string): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, token)
  } catch {
    // Storage can be blocked. The token then lasts until the page is closed.
  }
}

export function clearToken(): void {
  try {
    window.localStorage.removeItem(STORAGE_KEY)
  } catch {
    // Nothing to clear when storage is blocked.
  }
}
