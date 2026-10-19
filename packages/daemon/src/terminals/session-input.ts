import type { TerminalSize } from '@natamux/core'
import { flushOutput, type SessionContext } from './session-output.js'
import { ENTER_AFTER_PASTE_MS, INPUT_GAP_MS } from './terminal-limits.js'
import type { ClientState, Session, TerminalClient } from './terminal-types.js'

export function sameSize(a: TerminalSize, b: TerminalSize): boolean {
  return a.cols === b.cols && a.rows === b.rows
}

// A Vietnamese input method types "a", then erases it with DEL and types "á". Claude Code drops
// the text that it reads in one piece with a DEL, and pieces sent at the same moment are read
// together. So DEL and other input go to the shell apart, INPUT_GAP_MS from each other. Typing
// without DEL, or holding Backspace, is not slowed down.
export function write(context: SessionContext, session: Session, data: string): void {
  for (const piece of data.match(/\x7f+|[^\x7f]+/g) ?? []) {
    session.pending.push({ data: piece, kind: piece.startsWith('\x7f') ? 'erase' : 'text' })
  }
  flush(context, session)
}

export function flush(context: SessionContext, session: Session): void {
  if (session.writeTimer !== undefined) {
    return
  }
  for (let part = session.pending[0]; part !== undefined; part = session.pending[0]) {
    const { data, kind } = part
    const last = session.lastWrite
    const gap = kind === 'submit' ? ENTER_AFTER_PASTE_MS : INPUT_GAP_MS
    const wait = last && last.kind !== kind ? last.at + gap - context.now() : 0
    if (wait > 0) {
      session.writeTimer = setTimeout(() => {
        session.writeTimer = undefined
        flush(context, session)
      }, wait)
      return
    }
    session.pending.shift()
    session.pty.write(data)
    session.lastWrite = { at: context.now(), kind }
  }
}

// Makes a client the one used last, and the one that claimed the size.
export function touch(session: Session, client: TerminalClient): ClientState | undefined {
  const state = session.clients.get(client)
  if (state) {
    session.clients.delete(client)
    session.clients.set(client, state)
    state.claimed = true
  }
  return state
}

// Only the daemon's screen pauses a shell; a slow client never does.
export function setPaused(session: Session, paused: boolean): void {
  if (session.paused === paused) {
    return
  }
  session.paused = paused
  if (paused) {
    session.pty.pause()
  } else {
    session.pty.resume()
  }
}

// The shell is told only when the size really changes. Output drawn for the old size goes out
// first.
export function setSize(context: SessionContext, session: Session, size: TerminalSize): void {
  if (!sameSize(session.size, size)) {
    flushOutput(context, session)
    session.size = size
    session.pty.resize(size.cols, size.rows)
    session.screen.resize(size)
  }
}
