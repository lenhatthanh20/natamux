import { writeFileSync } from 'node:fs'
import { app, BrowserWindow, nativeTheme, shell, type WebContents } from 'electron'
import { log } from '../daemon/process.js'
import { checkReleaseSignature } from '../release-signature.js'
import { runSmoke } from '../smoke.js'
import { isDashboardUrl, loadingPageUrl, opensInBrowser } from './navigation.js'
import type { AppContext } from './state.js'
import { windowOptions } from './window-options.js'

// onDashboardLoad runs once, when the dashboard (not the loading page) has loaded.
export function createWindow(
  ctx: AppContext,
  onDashboardLoad?: (contents: WebContents) => void,
): void {
  const { state, marks } = ctx
  const dark = nativeTheme.shouldUseDarkColors
  const origin = new URL(state.dashboardUrl).origin
  const created = new BrowserWindow(windowOptions(ctx.preloadFile, dark))
  state.window = created
  marks.mark('window created')
  // By origin, not by the exact loading page URL: Chromium may rewrite a data: URL.
  const onDashboard = (): boolean => created.webContents.getURL().startsWith(`${origin}/`)
  created.once('ready-to-show', () => {
    marks.mark('ready-to-show')
    created.show()
  })
  created.webContents.on('did-finish-load', () => {
    if (!state.marksLogged && onDashboard()) {
      state.marksLogged = true
      marks.mark('did-finish-load')
      log(ctx, marks.line())
    }
  })
  if (onDashboardLoad !== undefined) {
    const loaded = () => {
      if (onDashboard()) {
        created.webContents.off('did-finish-load', loaded)
        onDashboardLoad(created.webContents)
      }
    }
    created.webContents.on('did-finish-load', loaded)
  }
  created.on('closed', () => {
    state.window = undefined
  })

  created.webContents.setWindowOpenHandler(({ url }) => {
    if (opensInBrowser(url)) {
      void shell.openExternal(url)
    }
    return { action: 'deny' }
  })
  created.webContents.on('will-navigate', (event, url) => {
    if (!isDashboardUrl(url, origin)) {
      event.preventDefault()
      if (opensInBrowser(url)) {
        void shell.openExternal(url)
      }
    }
  })
  if (process.env.NATAMUX_DESKTOP_SMOKE === '1') {
    // A check for CI and the codespace: the dashboard loads signed in, a terminal and an agent work
    // (smoke.ts), then the app quits. The exit code tells: on Windows the app's output may not reach
    // the console.
    created.webContents.on('did-finish-load', async () => {
      // The loading page finishes first.
      if (!onDashboard()) {
        return
      }
      await new Promise((resolve) => setTimeout(resolve, 4000))
      const signedIn: unknown = await created.webContents.executeJavaScript(
        `document.querySelector('[aria-label="Daemon token"]') === null && document.querySelector('#root')?.childElementCount > 0`,
      )
      console.log(`natamux-desktop: ${created.webContents.getURL()} signed in: ${String(signedIn)}`)
      const shot = process.env.NATAMUX_DESKTOP_SMOKE_SHOT
      if (shot) {
        writeFileSync(shot, (await created.webContents.capturePage()).toPNG())
      }
      let passed = signedIn === true
      if (passed) {
        // Also in the daemon log, which CI reads on Windows.
        let result = 'natamux-desktop: smoke passed, the terminal and the agent work'
        try {
          await runSmoke(state.dashboardUrl, state.daemonToken, process.env, process.platform)
          result += `, ${checkReleaseSignature()}`
        } catch (error) {
          passed = false
          const message = error instanceof Error ? error.message : String(error)
          result = `natamux-desktop: smoke failed: ${message}`
        }
        console.log(result)
        log(ctx, `${result}\n`)
      }
      state.quitConfirmed = true
      state.daemon?.kill()
      app.exit(passed ? 0 : 1)
    })
  }
  if (state.daemonReady) {
    void created.loadURL(`${state.dashboardUrl}/`)
  } else {
    // The dashboard load cuts this one short when the daemon is quick, and that rejects.
    created.loadURL(loadingPageUrl(dark)).catch(() => undefined)
  }
}
