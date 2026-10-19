import { app, dialog, Menu, type MenuItemConstructorOptions, shell } from 'electron'
import { addProject } from '../daemon/daemon-access.js'
import { pickRepository } from '../daemon/process.js'
import { loadSettings, saveSettings } from './settings.js'
import type { AppContext } from './state.js'

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

export function buildMenu(ctx: AppContext): void {
  const { state, awake, settingsFile, logFile } = ctx
  const mac = process.platform === 'darwin'
  const updates = state.updater?.menuItems() ?? []
  const template: MenuItemConstructorOptions[] = [
    ...(mac
      ? [
          {
            label: app.name,
            submenu: [
              { role: 'about' },
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
          click: (item) => {
            state.keepAwake = item.checked
            saveSettings(settingsFile, {
              ...loadSettings(settingsFile),
              keepAwake: state.keepAwake,
            })
            awake.update(state.runningAgents > 0 && state.keepAwake)
          },
        },
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
