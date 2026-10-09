// The window shows the dashboard only. Any other address opens in the user's browser, and only
// web and mail addresses open at all: a page must not start programs through other schemes.

export function isDashboardUrl(url: string, origin: string): boolean {
  try {
    return new URL(url).origin === origin
  } catch {
    return false
  }
}

export function opensInBrowser(url: string): boolean {
  try {
    return ['http:', 'https:', 'mailto:'].includes(new URL(url).protocol)
  } catch {
    return false
  }
}
