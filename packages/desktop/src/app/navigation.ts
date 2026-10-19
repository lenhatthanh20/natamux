// The window shows the dashboard only. Any other address opens in the user's browser, and only
// web and mail addresses open at all: a page must not start programs through other schemes.

// The page the window shows while the daemon starts. A data: URL needs no file in the package.
export function loadingPageUrl(dark: boolean): string {
  const html = `<!doctype html><title>Natamux</title><body style="margin:0;background:${dark ? '#18181b' : '#ffffff'}">`
  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`
}

export function isLoadingPage(url: string): boolean {
  return url === loadingPageUrl(true) || url === loadingPageUrl(false)
}

// Only the dashboard and the exact loading page.
export function isDashboardUrl(url: string, origin: string): boolean {
  if (isLoadingPage(url)) {
    return true
  }
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
