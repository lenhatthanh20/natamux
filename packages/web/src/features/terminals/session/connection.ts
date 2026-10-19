import { HEARTBEAT_MS, type TerminalClientMessage } from '@natamux/core'
import type { Terminal } from '@xterm/xterm'
import { closeSocket } from '../../../lib/live'
import { parseTerminalMessage, reconnectDelay, type TerminalConnectQuery } from '../terminal'
import { FINAL_CLOSE_CODES } from './constants'
import type { CatchUp, SessionCore, TerminalSessionState } from './types'

const pageHidden = () => document.visibilityState === 'hidden'

// An ended shell keeps its last screen to read, but must not look like it still takes keys.
function readOnly(terminal: Terminal): void {
  terminal.options.disableStdin = true
  terminal.options.cursorBlink = false
  terminal.write('\x1b[?25l')
}

// What one session sends on its socket.
export function createSender(s: SessionCore) {
  const { terminal } = s

  // Returns false when the socket is not open and nothing went out. After the end nothing goes
  // out, also keys typed before xterm stops taking them.
  const send = (message: TerminalClientMessage): boolean => {
    if (s.ended || s.socket === null || s.socket.readyState !== WebSocket.OPEN) {
      return false
    }
    s.socket.send(JSON.stringify(message))
    return true
  }
  // A size that could not go out (no socket) is not taken as sent, so a fit back to the old size
  // after a reconnect sends nothing.
  const sendSize = (always = false) => {
    if (s.catchUp !== null) {
      return
    }
    if (always || terminal.cols !== s.sentSize.cols || terminal.rows !== s.sentSize.rows) {
      const size = { cols: terminal.cols, rows: terminal.rows }
      if (send({ type: 'resize', ...size })) {
        s.sentSize = size
        s.claimed = true
        s.redrawWanted ||= s.covered
      }
    }
  }
  const claimSize = () => sendSize(true)
  const sendInput = (data: string) => {
    if (!s.claimed) {
      claimSize()
    }
    send({ type: 'input', data })
  }
  // Flow control on the daemon reads it (PR 4).
  const ack = (at: number) => {
    s.acked = at
    send({ type: 'ack', offset: at })
  }

  // claim: the terminal itself came on screen in the page, so the message carries its size and
  // takes the shell's size. The page coming back never claims (spec 2.1).
  const sendVisible = (claim: boolean) => {
    if (s.socket === null || s.socket.readyState !== WebSocket.OPEN) {
      return
    }
    const visible = s.onScreen && document.visibilityState !== 'hidden'
    if (claim && visible && s.catchUp !== null) {
      // Its size is the snapshot's until the catch-up ends.
      s.claimAfterCatchUp = true
    } else if (claim && visible) {
      s.sentVisible = true
      s.claimed = true
      s.sentSize = { cols: terminal.cols, rows: terminal.rows }
      send({ type: 'visible', visible: true, ...s.sentSize })
      return
    }
    if (visible !== s.sentVisible) {
      s.sentVisible = visible
      send({ type: 'visible', visible })
    }
  }

  return { send, sendSize, claimSize, sendInput, ack, sendVisible }
}

// The socket of one session: connect, heartbeat and reconnect (spec 5).
export function createConnection(
  s: SessionCore,
  deps: {
    connect: (query: TerminalConnectQuery) => Promise<WebSocket>
    random: () => number
    update: (change: Partial<TerminalSessionState>) => void
    end: () => void
    send: (message: TerminalClientMessage) => boolean
    sendSize: () => void
    sendVisible: (claim: boolean) => void
    hideScreen: () => void
    beginCatchUp: (kind: CatchUp['kind']) => void
    finishCatchUp: (at: number | null) => void
    output: (bytes: Uint8Array) => void
  },
) {
  const {
    connect,
    random,
    update,
    end,
    send,
    sendSize,
    sendVisible,
    hideScreen,
    beginCatchUp,
    finishCatchUp,
    output,
  } = deps
  const { terminal } = s
  // The shell this terminal shows, from `hello`.
  let shellId: string | null = null
  // A socket of this session opened once. Before that, a close is a failed start, not a loss.
  let everOpened = false
  // A connect or a socket that is not open yet is on its way.
  let connecting = false
  let tries = 0
  let retryTimer: ReturnType<typeof setTimeout> | undefined
  let pingTimer: ReturnType<typeof setInterval> | undefined
  let pongTimer: ReturnType<typeof setTimeout> | undefined

  const onPageVisibility = () => {
    sendVisible(false)
    if (pageHidden()) {
      stopHeartbeat()
      clearTimeout(retryTimer)
      retryTimer = undefined
    } else {
      startHeartbeat()
      retry()
    }
  }

  const stopHeartbeat = () => {
    clearInterval(pingTimer)
    clearTimeout(pongTimer)
    pingTimer = undefined
    pongTimer = undefined
  }
  // The deadline is set before the ping goes out, so a pong can never come before it.
  const ping = () => {
    if (s.socket?.readyState === WebSocket.OPEN) {
      pongTimer ??= setTimeout(stale, 2 * HEARTBEAT_MS)
      send({ type: 'ping' })
    }
  }
  // Only while the page is visible: a page in the background has its timers slowed down, and a
  // pong would seem late.
  const startHeartbeat = () => {
    stopHeartbeat()
    if (!pageHidden() && s.socket?.readyState === WebSocket.OPEN) {
      pingTimer = setInterval(ping, HEARTBEAT_MS)
    }
  }

  // Tries again after the backoff delay. A hidden page does not: it tries when it comes back.
  const planRetry = () => {
    clearTimeout(retryTimer)
    retryTimer = undefined
    if (s.disposed || pageHidden()) {
      return
    }
    retryTimer = setTimeout(retry, reconnectDelay(tries, random()))
    tries += 1
  }
  // Also at once on `online` and when the page comes back, once, even with a timer planned.
  const retry = () => {
    clearTimeout(retryTimer)
    retryTimer = undefined
    if (s.disposed || connecting || s.state.status !== 'reconnecting' || pageHidden()) {
      return
    }
    open(s.onScreen)
  }
  const cancelRetry = () => clearTimeout(retryTimer)

  // The socket closed, or went silent, while the shell may still run (spec 5). Typing queued by a
  // catch-up stays queued: the catch-up of the next socket sends it.
  const lost = () => {
    s.socket = null
    connecting = false
    stopHeartbeat()
    if (s.catchUp !== null) {
      clearTimeout(s.catchUp.timer)
    }
    update({ status: 'reconnecting' })
    planRetry()
  }
  // No pong in time: the connection is dead even if the browser has not noticed yet.
  const stale = () => {
    const old = s.socket
    if (old === null) {
      return
    }
    old.onopen = null
    old.onmessage = null
    old.onclose = null
    closeSocket(old)
    lost()
  }

  const listen = (opened: WebSocket) => {
    s.socket = opened
    // Output comes in binary frames. An ArrayBuffer can be written at once, a Blob cannot.
    opened.binaryType = 'arraybuffer'
    let wasOpen = false
    opened.onopen = () => {
      wasOpen = true
      everOpened = true
      connecting = false
      update({ status: 'open' })
      // The page may have been hidden or shown while the socket was opening.
      sendVisible(false)
      // The terminal may have been fitted while the socket was opening.
      if (s.claimed) {
        sendSize()
      }
      startHeartbeat()
    }
    opened.onmessage = (event) => {
      // typeof, not instanceof: a buffer from another realm (jsdom in tests) fails instanceof.
      if (typeof event.data !== 'string') {
        output(new Uint8Array(event.data))
        return
      }
      const message = parseTerminalMessage(event.data)
      if (message?.type === 'hello') {
        shellId = message.shellId
        // The daemon answered: the next loss starts the backoff from the first delay.
        tries = 0
      } else if (message?.type === 'pong') {
        clearTimeout(pongTimer)
        pongTimer = undefined
      } else if (message?.type === 'snapshot') {
        beginCatchUp('snapshot')
        hideScreen()
        const { cols, rows } = message
        // In xterm's write queue: the bytes before are parsed first, the snapshot after. The
        // resize is local only (sendSize sends nothing during a catch-up).
        terminal.write('', () => {
          if (!s.disposed) {
            terminal.reset()
            terminal.resize(cols, rows)
          }
        })
      } else if (message?.type === 'resume') {
        beginCatchUp('resume')
      } else if (message?.type === 'replayed') {
        const at = message.offset
        // The offset now, in the order of the frames: live output that comes before xterm parsed
        // the catch-up counts from here. Left uncounted, the client would ack too little forever,
        // and the daemon would never catch it up again once it is behind.
        s.offset = at
        s.offsetKnown = true
        if (s.catchUp !== null) {
          s.catchUp.replayed = true
        }
        // The catch-up ends once xterm parsed the last chunk: the screen stays hidden until then,
        // and an ack means parsed. After the 10 s failure path this only acks. A catch-up of a
        // later socket is not this one's to end.
        const current = s.catchUp
        terminal.write('', () => {
          if (s.catchUp === current) {
            finishCatchUp(at)
          }
        })
      } else if (message?.type === 'exit') {
        finishCatchUp(null)
        terminal.write('\r\n')
        readOnly(terminal)
        update({ status: 'ended', exitCode: message.code })
        end()
      } else if (message?.type === 'error') {
        finishCatchUp(null)
        update({ status: 'error', error: message.message })
        end()
      }
    }
    opened.onclose = (event) => {
      if (s.disposed || s.socket !== opened) {
        return
      }
      // After `exit` or `error` the session is over; after a final code a retry would not help.
      if (!s.ended && everOpened && !FINAL_CLOSE_CODES.includes(event.code)) {
        lost()
        return
      }
      s.socket = null
      connecting = false
      stopHeartbeat()
      if (s.ended) {
        return
      }
      finishCatchUp(null)
      // A close before the open is a failed handshake, not a shell that ended.
      if (wasOpen) {
        readOnly(terminal)
        update({ status: 'ended' })
      } else {
        update({ status: 'error', error: s.state.error ?? 'Could not connect to the terminal.' })
      }
      end()
    }
  }

  // The first socket asks for the shell as it is, and starts it when none runs. A later one
  // resumes the shell it had: from its offset when known, else with a snapshot (spec 5). A resume
  // never takes the shell's size; its next claim sends the size.
  const open = (onScreenNow: boolean) => {
    connecting = true
    const visible = onScreenNow && !pageHidden()
    s.claimed = visible && shellId === null
    s.sentVisible = visible
    const query: TerminalConnectQuery = { cols: terminal.cols, rows: terminal.rows, visible }
    if (shellId !== null) {
      query.shellId = shellId
      if (s.offsetKnown) {
        query.offset = s.offset
      }
    }
    connect(query)
      .then((opened) => {
        if (s.disposed) {
          closeSocket(opened)
          return
        }
        listen(opened)
      })
      .catch((reason: unknown) => {
        if (s.disposed) {
          return
        }
        connecting = false
        if (everOpened) {
          planRetry()
          return
        }
        update({
          status: 'error',
          error: reason instanceof Error ? reason.message : String(reason),
        })
        end()
      })
  }

  return { open, retry, cancelRetry, onPageVisibility, stopHeartbeat }
}
