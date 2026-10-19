// The desktop app's own settings. autoUpdate is missing when the app cannot update itself.
export type DesktopSettings = { keepAwake: boolean; autoUpdate?: boolean }

// What the desktop app's preload gives the page (packages/desktop/src/preload.cts).
export type DesktopBridge = {
  pickFolder: () => Promise<string | undefined>
  // Optional, so a test can stub only what it uses.
  reportStatus?: (status: { needsYou: number; running: number }) => void
  // The Settings item of the OS menu. Returns the call that stops listening.
  onOpenSettings?: (callback: () => void) => () => void
  getDesktopSettings?: () => Promise<DesktopSettings | undefined>
  setDesktopSetting?: (change: Partial<DesktopSettings>) => Promise<void>
}

declare global {
  interface Window {
    // Set by the desktop app's preload. A plain browser tab has none.
    natamux?: DesktopBridge
  }
}
