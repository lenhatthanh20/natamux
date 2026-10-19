import { scrollbackLines } from '@natamux/core'
import { FitAddon } from '@xterm/addon-fit'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from '@xterm/xterm'
import { keyBelongsToShell } from '../../../lib/dispatch'
import { pageTheme, type ResolvedTheme, uiZoom } from '../../../lib/theme'
import { type ResolvedTerminalFont, terminalFont } from '../terminal-font'
import { TERMINAL_LOOK, terminalTheme } from '../terminal-theme'
import { REDRAW_MAX_MS, REDRAW_QUIET_MS, REFLOW_WAIT_ROWS } from './constants'
import type { SessionCore, TerminalSessionState } from './types'

// A terminal that is hidden, not open yet or already disposed has no size, and xterm throws
// ("reading 'dimensions'") while it measures nothing. The next resize fits it.
export function safeFit(fit: FitAddon): void {
  try {
    fit.fit()
  } catch {
    // Nothing to fit yet.
  }
}

// check() throws on a font string it cannot parse; load() then fails too, and is ignored.
function fontLoaded(spec: string): boolean {
  try {
    return document.fonts?.check(spec) === true
  } catch {
    return false
  }
}

export function fontKey(font: ResolvedTerminalFont, zoom: number): string {
  return `${font.stack}|${font.size * zoom}`
}

// xterm with its addons, in its own parent element.
export function createXterm(
  firstFont: ResolvedTerminalFont,
  firstZoom: number,
  theme: ResolvedTheme,
) {
  const terminal = new Terminal({
    // The search addon marks its matches with decorations, and Unicode 11 widths are set
    // through terminal.unicode: both are proposed APIs.
    allowProposedApi: true,
    ...TERMINAL_LOOK,
    fontFamily: firstFont.stack,
    fontSize: firstFont.size * firstZoom,
    scrollback: scrollbackLines(80),
    theme: terminalTheme(theme),
  })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  // Only web addresses become links, and they open in a new tab.
  terminal.loadAddon(
    new WebLinksAddon((event, uri) => {
      event.preventDefault()
      window.open(uri, '_blank', 'noopener,noreferrer')
    }),
  )
  // The keys of the app (F6, Cmd chords on a Mac, Ctrl+Shift chords elsewhere) are not for the
  // shell. Returning false lets the key event go on to the page, see lib/dispatch.ts.
  terminal.attachCustomKeyEventHandler((event) => keyBelongsToShell(event))
  // Emoji and wide signs take two cells, as in the daemon's screen (terminal-service.ts).
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'
  // xterm's own parent. It moves between a pane and the hidden holder, and FitAddon measures it,
  // so it has the classes of the box the terminal fills.
  const host = document.createElement('div')
  host.className = 'min-h-0 flex-1 overflow-hidden'
  // The fade runs once; left on, it would run again each time the element moves.
  host.addEventListener('animationend', (event) => {
    if (event.target === host) {
      host.classList.remove('hm-fade-in')
    }
  })
  return { terminal, fit, host }
}

// The screen, the size and the look of one session: what changes with the pane and the settings.
export function createLayout(
  s: SessionCore,
  deps: {
    update: (change: Partial<TerminalSessionState>) => void
    sendSize: () => void
    sendVisible: (claim: boolean) => void
  },
) {
  const { update, sendSize, sendVisible } = deps
  const { terminal, fit, host } = s
  // See `covered` in SessionCore.
  let catchUpHidden = false
  let quietTimer: ReturnType<typeof setTimeout> | undefined
  let frame: number | null = null

  // Scroll state goes to React at most once per frame, and never while hidden: output of a
  // hidden terminal would re-render a view for nothing.
  const updateScrolled = () => {
    if (!s.onScreen || frame !== null || s.catchUp !== null) {
      return
    }
    frame = requestAnimationFrame(() => {
      frame = null
      const buffer = terminal.buffer.active
      update({ scrolledUp: buffer.viewportY < buffer.baseY })
    })
  }

  // A catch-up hides with visibility; the textarea stays visible, so it keeps the focus and typed
  // keys are queued, not lost. A cover uses opacity: the pane still takes a click, which focuses
  // the terminal (a hidden element gets no click).
  const applyScreen = () => {
    host.style.visibility = catchUpHidden ? 'hidden' : ''
    host.style.opacity = s.covered ? '0' : ''
    if (terminal.textarea !== undefined) {
      terminal.textarea.style.visibility = catchUpHidden ? 'visible' : ''
    }
  }
  const hideScreen = () => {
    catchUpHidden = true
    applyScreen()
    update({ catchingUp: true })
  }
  const showScreen = () => {
    catchUpHidden = false
    applyScreen()
    update({ catchingUp: false })
  }

  // The pane changes size: the frame stays, the screen goes until the shell drew for the new size,
  // so the reflow and the program's redraw are not seen.
  const stopRedrawWait = () => {
    clearTimeout(quietTimer)
    clearTimeout(s.redrawTimer)
    quietTimer = undefined
    s.redrawTimer = undefined
  }
  const cover = () => {
    stopRedrawWait()
    if (!s.covered) {
      s.covered = true
      host.classList.remove('hm-fade-in')
      applyScreen()
    }
  }
  // fade: the screen comes back in the same place; not when the element moves.
  const uncover = (fade = true) => {
    stopRedrawWait()
    s.redrawWanted = false
    if (!s.covered) {
      return
    }
    s.covered = false
    applyScreen()
    if (fade && s.onScreen && !catchUpHidden) {
      // Read in between, so the animation starts again.
      void host.offsetWidth
      host.classList.add('hm-fade-in')
    }
  }
  // Each output moves the end REDRAW_QUIET_MS later; redrawTimer is the latest end.
  const waitForQuiet = () => {
    clearTimeout(quietTimer)
    quietTimer = setTimeout(() => uncover(), REDRAW_QUIET_MS)
  }

  // During a snapshot the xterm keeps the snapshot's size; it fits back when the catch-up ends.
  const fitNow = () => {
    if (s.onScreen && s.catchUp?.kind !== 'snapshot') {
      safeFit(fit)
    }
  }

  // While the pane changes size: the rows follow at once, the columns too in a short buffer.
  const resizeNow = () => {
    if (!s.onScreen || s.catchUp?.kind === 'snapshot') {
      return
    }
    let size: { cols: number; rows: number } | undefined
    try {
      size = fit.proposeDimensions()
    } catch {
      // Nothing to measure yet, see safeFit.
      return
    }
    // A pane with no computed size (display: none) measures NaN, which xterm's resize throws on.
    if (size === undefined || Number.isNaN(size.cols) || Number.isNaN(size.rows)) {
      return
    }
    if (size.cols !== terminal.cols || size.rows !== terminal.rows) {
      cover()
    }
    const cols = terminal.buffer.normal.length < REFLOW_WAIT_ROWS ? size.cols : terminal.cols
    s.holdSize = true
    try {
      terminal.resize(cols, size.rows)
    } finally {
      s.holdSize = false
    }
  }

  // The pane settled: the columns catch up, and the shell gets the size once. The screen shows
  // after the shell's redraw, or at once when the shell got no new size.
  const settle = () => {
    if (!s.onScreen) {
      // Off screen (watchVisible): nothing to see, and no settle may come to end the cover.
      uncover(false)
      return
    }
    fitNow()
    sendSize()
    if (!s.covered) {
      return
    }
    if (s.redrawWanted) {
      s.redrawWanted = false
      s.redrawTimer = setTimeout(() => uncover(), REDRAW_MAX_MS)
      waitForQuiet()
    } else {
      uncover()
    }
  }

  // Setting another family first makes xterm measure the cells again, even for the same family.
  const remeasure = (stack: string) => {
    terminal.options.fontFamily = 'monospace'
    terminal.options.fontFamily = stack
  }

  // The web font may still be loading when the terminal measures its cells. Once it is loaded,
  // the terminal measures again; a hidden one at its next show, as it cannot measure now.
  const whenFontLoads = (font: ResolvedTerminalFont) => {
    const spec = `${Math.round(font.size)}px ${font.stack}`
    // Already loaded: the cells were measured with it, and measuring again would only redraw
    // every glyph, a long task when a view opens several terminals at once.
    if (fontLoaded(spec)) {
      return
    }
    document.fonts
      ?.load(spec)
      .then(() => {
        if (s.disposed) {
          return
        }
        if (s.onScreen) {
          remeasure(terminalFont().stack)
          fitNow()
        } else {
          s.measuredFont = ''
        }
      })
      .catch(() => undefined)
  }

  // Returns true when the font changed, so the caller fits. Call only on screen.
  const applyFont = (): boolean => {
    const font = terminalFont()
    const zoom = uiZoom()
    const key = fontKey(font, zoom)
    if (key === s.measuredFont) {
      return false
    }
    s.measuredFont = key
    remeasure(font.stack)
    terminal.options.fontSize = font.size * zoom
    whenFontLoads(font)
    return true
  }

  const applyTheme = () => {
    const next = pageTheme()
    if (next !== s.theme) {
      s.theme = next
      terminal.options.theme = terminalTheme(next)
    }
  }

  // A hidden terminal takes the font at its next show: it cannot measure cells while hidden,
  // and one change on show looks the same.
  const applySettings = () => {
    if (s.disposed) {
      return
    }
    applyTheme()
    if (s.onScreen && applyFont()) {
      fitNow()
    }
  }

  const setOnScreen = (value: boolean) => {
    if (s.disposed || value === s.onScreen) {
      return
    }
    // Before onScreen changes: a renderer taken on show then needs no fit of its own, as the fit
    // below covers it.
    s.gpu?.setVisible(value)
    s.onScreen = value
    if (value) {
      applyTheme()
      applyFont()
      fitNow()
      sendVisible(true)
      updateScrolled()
    } else {
      if (frame !== null) {
        cancelAnimationFrame(frame)
        frame = null
      }
      sendVisible(false)
    }
  }

  return {
    updateScrolled,
    hideScreen,
    showScreen,
    uncover,
    waitForQuiet,
    fitNow,
    resizeNow,
    settle,
    whenFontLoads,
    applySettings,
    setOnScreen,
  }
}
