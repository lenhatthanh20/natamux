import { app, BrowserWindow, nativeTheme } from 'electron'
import {
  checkingView,
  type PopupView,
  popupWindowOptions,
  showScript,
  updatePopupUrl,
} from './update-popup.js'

// A quick check would only flash the popup.
const POPUP_MIN_MS = 600

// The popup of a manual check.
export class PopupController {
  // Undefined when closed, also by the user with Hide.
  private popup: BrowserWindow | undefined
  private openedAt = 0
  private loaded = false
  private view: PopupView | undefined
  private shown = ''

  // The popup sits over the window that parent() gives.
  constructor(private readonly parent: () => BrowserWindow | undefined) {}

  open(): void {
    this.view = checkingView(app.getVersion())
    if (this.popup !== undefined) {
      this.send()
      return
    }
    const parent = this.parent()
    const popup = new BrowserWindow({
      ...popupWindowOptions(nativeTheme.shouldUseDarkColors, parent?.getBounds()),
      parent,
    })
    this.popup = popup
    this.openedAt = Date.now()
    this.loaded = false
    this.shown = ''
    popup.webContents.on('will-navigate', (event) => event.preventDefault())
    popup.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    popup.webContents.once('did-finish-load', () => {
      this.loaded = true
      this.send()
      popup.show()
    })
    popup.on('closed', () => {
      if (this.popup === popup) {
        this.popup = undefined
      }
    })
    popup.loadURL(updatePopupUrl()).catch(() => undefined)
  }

  // Does nothing when the popup is closed, so Hide stays hidden.
  update(view: PopupView): void {
    if (this.popup !== undefined) {
      this.view = view
      this.send()
    }
  }

  // Sends only a change: the download reports every chunk, the view changes once a MB.
  private send(): void {
    const popup = this.popup
    if (popup === undefined || !this.loaded || this.view === undefined) {
      return
    }
    const script = showScript(this.view)
    if (script !== this.shown) {
      this.shown = script
      popup.webContents.executeJavaScript(script).catch(() => undefined)
    }
  }

  async close(): Promise<void> {
    const popup = this.popup
    if (popup === undefined) {
      return
    }
    const wait = POPUP_MIN_MS - (Date.now() - this.openedAt)
    if (wait > 0) {
      await new Promise((resolve) => setTimeout(resolve, wait))
    }
    if (this.popup === popup) {
      this.popup = undefined
    }
    if (!popup.isDestroyed()) {
      popup.destroy()
    }
  }
}
