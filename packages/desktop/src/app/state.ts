import type { BrowserWindow, UtilityProcess } from 'electron'
import type { startupMarks } from '../daemon/startup.js'
import type { Updater } from '../update/updater.js'
import type { AwakeGuard } from './agent-status.js'

// Everything the main process changes while it runs.
export type AppState = {
  daemon: UtilityProcess | undefined
  daemonExited: boolean
  daemonToken: string
  dashboardUrl: string
  // False while the window shows the loading page and waits for the daemon.
  daemonReady: boolean
  marksLogged: boolean
  window: BrowserWindow | undefined
  quitConfirmed: boolean
  daemonStopWaited: boolean
  updater: Updater | undefined
  keepAwake: boolean
  // The last count the dashboard reported.
  runningAgents: number
  lastLines: string[]
}

// The state with what main.ts fixes at start: the paths, the marks and the awake hold.
export type AppContext = {
  state: AppState
  marks: ReturnType<typeof startupMarks>
  awake: AwakeGuard
  userData: string
  settingsFile: string
  logFile: string
  preloadFile: string
}
