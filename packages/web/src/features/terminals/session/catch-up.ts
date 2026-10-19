import { ACK_EVERY_BYTES } from '@natamux/core'
import { CATCH_UP_TIMEOUT_MS, HIDDEN_RESUME_BYTES } from './constants'
import { safeFit } from './layout'
import type { CatchUp, SessionCore } from './types'

// Output of the socket, and the snapshot or resume that brings the screen up to date (spec 6).
export function createCatchUp(
  s: SessionCore,
  deps: {
    ack: (at: number) => void
    sendSize: () => void
    sendVisible: (claim: boolean) => void
    sendInput: (data: string) => void
    hideScreen: () => void
    showScreen: () => void
    waitForQuiet: () => void
    updateScrolled: () => void
  },
) {
  const {
    ack,
    sendSize,
    sendVisible,
    sendInput,
    hideScreen,
    showScreen,
    waitForQuiet,
    updateScrolled,
  } = deps
  const { terminal } = s

  // at is the offset from `replayed`, or null when the catch-up failed or the shell ended. The
  // offset itself was taken when `replayed` came.
  const finishCatchUp = (at: number | null) => {
    if (s.disposed) {
      return
    }
    if (at !== null) {
      ack(at)
    }
    if (s.catchUp === null) {
      return
    }
    clearTimeout(s.catchUp.timer)
    const kind = s.catchUp.kind
    // Back to the pane's size. Its resize sends nothing: the catch-up is still on.
    if (s.onScreen) {
      safeFit(s.fit)
    }
    s.catchUp = null
    if (kind === 'snapshot') {
      terminal.scrollToBottom()
    }
    showScreen()
    if (s.claimed) {
      sendSize()
    } else {
      s.sentSize = { cols: terminal.cols, rows: terminal.rows }
    }
    if (s.claimAfterCatchUp) {
      s.claimAfterCatchUp = false
      sendVisible(true)
    }
    // Kept while the socket is down: the catch-up of the next socket sends them.
    if (s.socket?.readyState === WebSocket.OPEN) {
      for (const data of s.typed.splice(0)) {
        sendInput(data)
      }
    }
    updateScrolled()
  }

  const beginCatchUp = (kind: CatchUp['kind']) => {
    // Bytes of a catch-up are not counted: a socket lost now resumes with a snapshot.
    s.offsetKnown = false
    if (s.catchUp !== null) {
      clearTimeout(s.catchUp.timer)
    }
    const timer = setTimeout(() => finishCatchUp(null), CATCH_UP_TIMEOUT_MS)
    s.catchUp = { kind, bytes: 0, replayed: false, timer }
  }

  const output = (bytes: Uint8Array) => {
    if (s.catchUp !== null && !s.catchUp.replayed) {
      // Not counted: snapshot and live bytes come mixed, and `replayed` gives the offset.
      s.catchUp.bytes += bytes.byteLength
      if (s.catchUp.kind === 'resume' && s.catchUp.bytes > HIDDEN_RESUME_BYTES) {
        hideScreen()
      }
      terminal.write(bytes)
      return
    }
    s.offset += bytes.byteLength
    const at = s.offset
    if (s.redrawTimer !== undefined) {
      waitForQuiet()
    }
    terminal.write(bytes, () => {
      if (!s.disposed && at - s.acked >= ACK_EVERY_BYTES) {
        ack(at)
      }
    })
  }

  return { finishCatchUp, beginCatchUp, output }
}
