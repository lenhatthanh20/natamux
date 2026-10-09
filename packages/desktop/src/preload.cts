// Runs in the dashboard page before its scripts, so the page starts signed in. A sandboxed preload
// must be CommonJS.
import electron = require('electron')

// The dashboard keeps the daemon's token under this key (packages/web/src/lib/auth-storage.ts).
const STORAGE_KEY = 'helmsman.auth'

const daemonToken: unknown = electron.ipcRenderer.sendSync('helmsman:daemon-token')
if (typeof daemonToken === 'string' && daemonToken !== '') {
  try {
    window.localStorage.setItem(STORAGE_KEY, daemonToken)
  } catch {
    // Storage is off: the page asks for the token instead.
  }
}
