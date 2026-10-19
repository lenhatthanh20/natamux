// The window shows the dashboard only. Any other address opens in the user's browser, and only
// web and mail addresses open at all: a page must not start programs through other schemes.

// The 8-dash spinner of the dashboard (web ui/Spinner.tsx) as plain markup. web/index.html shows
// the same one at the same place until the dashboard draws, so the switch does not jump.
const DASHES = [1, 0.3, 0.3, 0.3, 0.36, 0.48, 0.64, 0.82]
  .map(
    (opacity, index) =>
      `<path d="M10.89 4.08A8 8 0 0 1 13.11 4.08" transform="rotate(${index * 45} 12 12)" opacity="${opacity}"/>`,
  )
  .join('')
const DOTS = [5, 12, 19].map((cx) => `<circle cx="${cx}" cy="12" r="2.1"/>`).join('')
const SPINNER = `<svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true"><g class="boot-dashes" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round">${DASHES}</g><g class="boot-dots" fill="currentColor">${DOTS}</g></svg>`

// The page the window shows while the daemon starts. A data: URL needs no file in the package.
export function loadingPageUrl(dark: boolean): string {
  const style = `body{margin:0;height:100vh;display:grid;place-items:center;background:${dark ? '#18181b' : '#ffffff'};color:${dark ? '#8d919a' : '#5f6570'}}svg{animation:spin 1s steps(8) infinite}@keyframes spin{to{transform:rotate(360deg)}}.boot-dots{display:none}@media (prefers-reduced-motion:reduce){svg{animation:none}.boot-dashes{display:none}.boot-dots{display:inline}}`
  const html = `<!doctype html><title>Natamux</title><style>${style}</style><body>${SPINNER}`
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
