// Runs in the dashboard page before its scripts, so the page starts signed in. A sandboxed preload
// must be CommonJS.
import electron = require('electron')

// The dashboard keeps the daemon's token under this key (packages/web/src/lib/auth-storage.ts).
const STORAGE_KEY = 'natamux.auth'

const daemonToken: unknown = electron.ipcRenderer.sendSync('natamux:daemon-token')
if (typeof daemonToken === 'string' && daemonToken !== '') {
  try {
    window.localStorage.setItem(STORAGE_KEY, daemonToken)
  } catch {
    // Storage is off: the page asks for the token instead.
  }
}

// Ghostty's font for the terminals (packages/web/src/lib/terminal-font.ts). A font the user
// picks in the dashboard comes first.
const GHOSTTY_FONT_KEY = 'natamux.ghostty-font'
const ghosttyFont: unknown = electron.ipcRenderer.sendSync('natamux:ghostty-font')
try {
  if (typeof ghosttyFont === 'string' && ghosttyFont !== '') {
    window.localStorage.setItem(GHOSTTY_FONT_KEY, ghosttyFont)
  } else {
    window.localStorage.removeItem(GHOSTTY_FONT_KEY)
  }
} catch {
  // Storage is off: the terminals use the built-in font.
}

// The page is isolated, so it gets this one function instead of ipcRenderer. Undefined: canceled.
electron.contextBridge.exposeInMainWorld('natamux', {
  pickFolder: (): Promise<string | undefined> => electron.ipcRenderer.invoke('natamux:pick-folder'),
})
