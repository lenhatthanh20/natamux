import { charBorder, splitFrames } from './screen-state.js'
import {
  CLIENT_HIGH_WATER,
  HIDDEN_BUDGET_BYTES,
  HIDDEN_BUDGET_WINDOW_MS,
  OUTPUT_COALESCE_MS,
  OUTPUT_FRAME_BYTES,
  OUTPUT_IDLE_BYTES,
  OUTPUT_RING_BYTES,
  RESYNC_QUIET_WINDOW_MS,
} from './terminal-limits.js'
import type { ClientState, Session } from './terminal-types.js'

// What the session helpers need from the service. `catchUp` is catchUpBehind, passed in so the
// output does not import the snapshot code that imports it.
export type SessionContext = {
  now: () => number
  catchUp: (session: Session) => void
}

export function ringSince(session: Session, offset: number): Buffer[] | undefined {
  if (offset < session.ringStart || offset > session.offset) {
    return undefined
  }
  let at = session.ringStart
  for (const [index, frame] of session.ring.entries()) {
    if (at === offset) {
      return session.ring.slice(index)
    }
    if (at > offset) {
      return undefined
    }
    at += frame.length
  }
  return at === offset ? [] : undefined
}

export function dropOldOutput(session: Session, now: number): void {
  while ((session.recent[0]?.at ?? now) <= now - RESYNC_QUIET_WINDOW_MS) {
    session.recentBytes -= session.recent.shift()?.bytes ?? 0
  }
}

// Output goes out in batches: one frame for many small chunks, see OUTPUT_COALESCE_MS.
export function queueOutput(context: SessionContext, session: Session, bytes: Buffer): void {
  if (bytes.length === 0) {
    return
  }
  const now = context.now()
  session.recent.push({ at: now, bytes: bytes.length })
  session.recentBytes += bytes.length
  dropOldOutput(session, now)
  session.batch.push(bytes)
  session.batchBytes += bytes.length
  if (session.batchBytes >= OUTPUT_FRAME_BYTES) {
    sendFullFrames(context, session)
  }
  if (
    session.batchBytes === 0 ||
    session.flushTimer !== undefined ||
    session.flushImmediate !== undefined
  ) {
    return
  }
  const idle = now - session.lastFlushAt >= OUTPUT_COALESCE_MS
  if (idle && session.batchBytes < OUTPUT_IDLE_BYTES) {
    session.flushImmediate = setImmediate(() => flushOutput(context, session))
  } else {
    session.flushTimer = setTimeout(() => flushOutput(context, session), OUTPUT_COALESCE_MS)
  }
}

// Sends the full frames of the batch and keeps the rest. A planned flush stays planned for it.
function sendFullFrames(context: SessionContext, session: Session): void {
  const all = Buffer.concat(session.batch, session.batchBytes)
  let start = 0
  while (all.length - start >= OUTPUT_FRAME_BYTES) {
    const end = charBorder(all, start, start + OUTPUT_FRAME_BYTES)
    sendFrame(context, session, all.subarray(start, end))
    start = end
  }
  session.batch = start < all.length ? [all.subarray(start)] : []
  session.batchBytes = all.length - start
  session.lastFlushAt = context.now()
}

// Also called before every control message and pty resize, so output comes before them.
export function flushOutput(context: SessionContext, session: Session): void {
  clearTimeout(session.flushTimer)
  clearImmediate(session.flushImmediate)
  session.flushTimer = undefined
  session.flushImmediate = undefined
  if (session.batchBytes === 0) {
    return
  }
  const bytes = Buffer.concat(session.batch, session.batchBytes)
  session.batch = []
  session.batchBytes = 0
  session.lastFlushAt = context.now()
  for (const frame of splitFrames(bytes)) {
    sendFrame(context, session, frame)
  }
}

function sendFrame(context: SessionContext, session: Session, frame: Buffer): void {
  const start = session.offset
  session.offset += frame.length
  session.ring.push(frame)
  session.ringBytes += frame.length
  while (session.ringBytes > OUTPUT_RING_BYTES && session.ring.length > 1) {
    const dropped = session.ring.shift()
    session.ringBytes -= dropped?.length ?? 0
    session.ringStart += dropped?.length ?? 0
  }
  for (const [client, state] of session.clients) {
    if (state.phase === 'live') {
      if (admits(context, state, frame.length)) {
        client.send(frame)
        state.sent = session.offset
      } else {
        // Dropped, not queued, so a slow or hidden client costs no memory.
        state.phase = 'behind'
        state.behindFrom = start
        state.behindSince = context.now()
        // A look soon, not now (this runs inside a flush): one whose acks stop needs a timer.
        session.resyncTimer ??= setTimeout(() => context.catchUp(session), 0)
      }
    } else if (state.waiting !== null) {
      if (state.waitingBytes + frame.length > CLIENT_HIGH_WATER) {
        state.behindFrom = start - state.waitingBytes
        state.behindSince = context.now()
        state.waiting = null
        state.waitingBytes = 0
      } else {
        state.waiting.push(frame)
        state.waitingBytes += frame.length
      }
    }
  }
}

// A live client takes a frame while it is within CLIENT_HIGH_WATER of its acks and, when hidden,
// while the frame fits the budget of the current window.
function admits(context: SessionContext, state: ClientState, bytes: number): boolean {
  if (state.sent + bytes - state.acked > CLIENT_HIGH_WATER) {
    return false
  }
  if (state.visible) {
    return true
  }
  const now = context.now()
  if (now - state.budgetFrom >= HIDDEN_BUDGET_WINDOW_MS) {
    const windows = Math.floor((now - state.budgetFrom) / HIDDEN_BUDGET_WINDOW_MS)
    state.budgetFrom += windows * HIDDEN_BUDGET_WINDOW_MS
    state.budgetBytes = 0
  }
  if (state.budgetBytes + bytes > HIDDEN_BUDGET_BYTES) {
    return false
  }
  state.budgetBytes += bytes
  return true
}
