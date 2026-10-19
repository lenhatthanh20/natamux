import { app } from 'electron'
import { buildMenu } from '../app/menu.js'
import { confirmStop } from '../app/quit.js'
import { autoUpdateOn, loadSettings, saveSettings } from '../app/settings.js'
import type { AppContext } from '../app/state.js'
import { Updater } from './updater.js'

// Only a packaged app updates: a run from the repository has no release to replace.
export function makeUpdater(ctx: AppContext): Updater | undefined {
  const { state, userData, settingsFile } = ctx
  if (!app.isPackaged || process.env.NATAMUX_DESKTOP_SMOKE === '1') {
    return undefined
  }
  return new Updater({
    userData,
    version: process.env.NATAMUX_UPDATE_AS_VERSION || app.getVersion(),
    autoUpdate: autoUpdateOn(loadSettings(settingsFile)),
    saveAutoUpdate: (on) =>
      saveSettings(settingsFile, { ...loadSettings(settingsFile), autoUpdate: on }),
    confirmRestart: () => confirmStop(state, 'Restart'),
    quit: () => {
      state.quitConfirmed = true
      app.quit()
    },
    changed: () => buildMenu(ctx),
    parent: () => state.window,
  })
}
