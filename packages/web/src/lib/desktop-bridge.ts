// What the desktop app's preload gives the page (packages/desktop/src/preload.cts).
export type DesktopBridge = {
  pickFolder: () => Promise<string | undefined>
  // Optional, so a test can stub only what it uses.
  reportStatus?: (status: { needsYou: number; running: number }) => void
}

declare global {
  interface Window {
    // Set by the desktop app's preload. A plain browser tab has none.
    natamux?: DesktopBridge
  }
}
