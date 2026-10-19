import type { BrowserWindowConstructorOptions } from 'electron'

export function windowOptions(preload: string, dark: boolean): BrowserWindowConstructorOptions {
  return {
    width: 1440,
    height: 900,
    minWidth: 800,
    minHeight: 560,
    show: false,
    title: 'Natamux',
    backgroundColor: dark ? '#18181b' : '#ffffff',
    // On macOS the click that activates the window would otherwise not reach the page, so a
    // terminal would need a second click to take the focus.
    acceptFirstMouse: true,
    webPreferences: {
      preload,
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      spellcheck: false,
    },
  }
}
