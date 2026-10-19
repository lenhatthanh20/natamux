import {
  app,
  dialog,
  Menu,
  type MenuItemConstructorOptions,
  shell,
  type WebContents,
} from 'electron'
import { addProject } from '../daemon/daemon-access.js'
import { pickRepository } from '../daemon/process.js'
import { loadSettings, saveSettings } from './settings.js'
import type { AppContext } from './state.js'
import { createWindow } from './window.js'

// The daemon tells the open dashboards, so the project shows up without a restart.
async function addProjectFromMenu(ctx: AppContext): Promise<void> {
  const { state } = ctx
  if (state.dashboardUrl === '' || state.daemonExited) {
    return
  }
  const dir = await pickRepository()
  if (dir === undefined) {
    return
  }
  const problem = await addProject(state.dashboardUrl, state.daemonToken, dir)
  if (problem !== undefined) {
    await dialog.showMessageBox({
      type: 'warning',
      message: `Natamux could not add ${dir}`,
      detail: problem,
    })
  }
}

// The dashboard opens its Settings dialog. With no window (macOS), one opens and is asked once the
// dashboard has loaded: the loading page has no dialog.
function openSettingsFromMenu(ctx: AppContext): void {
  const { state } = ctx
  if (!state.daemonReady || state.daemonExited) {
    return
  }
  const send = (contents: WebContents) => contents.send('natamux:open-settings')
  const window = state.window
  if (window === undefined) {
    createWindow(ctx, send)
    return
  }
  if (window.isMinimized()) {
    window.restore()
  }
  window.focus()
  send(window.webContents)
}

// The File menu tick and the dashboard's settings dialog. The menu is built again so its tick
// matches a change from the dialog.
export function setKeepAwake(ctx: AppContext, on: boolean): void {
  const { state, awake, settingsFile } = ctx
  state.keepAwake = on
  saveSettings(settingsFile, { ...loadSettings(settingsFile), keepAwake: on })
  awake.update(state.runningAgents > 0 && state.keepAwake)
  buildMenu(ctx)
}

export function buildMenu(ctx: AppContext): void {
  const { state, logFile } = ctx
  const mac = process.platform === 'darwin'
  const updates = state.updater?.menuItems() ?? []
  const settings: MenuItemConstructorOptions = {
    label: 'Settings…',
    accelerator: 'CmdOrCtrl+,',
    click: () => openSettingsFromMenu(ctx),
  }
  const template: MenuItemConstructorOptions[] = [
    ...(mac
      ? [
          {
            label: app.name,
            submenu: [
              { role: 'about' },
              settings,
              ...(updates.length > 0 ? [{ type: 'separator' as const }, ...updates] : []),
              { type: 'separator' },
              { role: 'services' },
              { type: 'separator' },
              { role: 'hide' },
              { role: 'hideOthers' },
              { role: 'unhide' },
              { type: 'separator' },
              { role: 'quit' },
            ] satisfies MenuItemConstructorOptions[],
          },
        ]
      : []),
    {
      label: 'File',
      submenu: [
        {
          label: 'Add Project…',
          accelerator: 'CmdOrCtrl+Shift+O',
          click: () => addProjectFromMenu(ctx),
        },
        { label: 'Show Daemon Log', click: () => void shell.openPath(logFile) },
        {
          label: 'Keep Computer Awake While Agents Run',
          type: 'checkbox',
          checked: state.keepAwake,
          click: (item) => setKeepAwake(ctx, item.checked),
        },
        ...(mac ? [] : [settings]),
        { type: 'separator' },
        mac ? { role: 'close' } : { role: 'quit' },
      ],
    },
    { role: 'editMenu' },
    { role: 'viewMenu' },
    { role: 'windowMenu' },
    ...(!mac && updates.length > 0 ? [{ label: 'Help', submenu: updates }] : []),
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}
