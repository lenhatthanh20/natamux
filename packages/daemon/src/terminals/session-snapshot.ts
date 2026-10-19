import { ACK_EVERY_BYTES, type TerminalSize } from '@natamux/core'
import { splitFrames } from './screen-state.js'
import { sameSize } from './session-input.js'
import { dropOldOutput, flushOutput, ringSince, type SessionContext } from './session-output.js'
import {
  ACK_STALL_MS,
  RESYNC_MAX_WAIT_MS,
  RESYNC_QUIET_WINDOW_MS,
  SERIALIZE_SLICE_MS,
  SNAPSHOT_SEND_BYTES,
} from './terminal-limits.js'
import type { ClientState, Session, SnapshotJob, TerminalClient } from './terminal-types.js'

// Sends a client that comes back the output since `offset`, when the ring still has all of it.
// False when it does not (too old, in the future, inside a frame, or no offset): the client then
// needs a snapshot.
export function resume(
  context: SessionContext,
  session: Session,
  client: TerminalClient,
  state: ClientState,
  offset: number | undefined,
): boolean {
  // The client is in the snapshot phase with nothing waiting, so the flush skips it and the ring
  // gets the pending output.
  flushOutput(context, session)
  const missed = offset === undefined ? undefined : ringSince(session, offset)
  if (offset === undefined || missed === undefined) {
    return false
  }
  client.send({ type: 'resume', offset })
  for (const frame of missed) {
    client.send(frame)
  }
  client.send({ type: 'replayed', offset: session.offset })
  state.sent = session.offset
  state.acked = session.offset
  state.phase = 'live'
  return true
}

// A new client gets the running snapshot when its cut is right for it too: no output since the
// cut, the same size, and no frame sent yet. Otherwise it takes the next one, which all such
// clients share.
export function requestSnapshot(
  context: SessionContext,
  session: Session,
  client: TerminalClient,
  state: ClientState,
): void {
  const job = session.snapshot
  if (job === undefined) {
    startSnapshot(context, session, new Set([client]))
    return
  }
  const sameCut =
    job.cut === session.offset && session.batchBytes === 0 && sameSize(job.size, session.size)
  if (!job.sending && (job.cut === -1 || sameCut)) {
    job.clients.add(client)
    if (job.cut !== -1) {
      state.waiting = []
    }
  } else {
    // Its output is dropped until the next cut; that snapshot has it.
    session.nextSnapshot.add(client)
  }
}

function startSnapshot(
  context: SessionContext,
  session: Session,
  clients: Set<TerminalClient>,
): void {
  let finish = (): void => undefined
  const done = new Promise<void>((resolve) => {
    finish = () => resolve()
  })
  const job: SnapshotJob = { clients, cut: -1, size: session.size, sending: false, done, finish }
  session.snapshot = job
  session.screen.snapshot({
    cut: () => {
      flushOutput(context, session)
      job.cut = session.offset
      job.size = session.size
      // Output from here on is not in the snapshot.
      for (const client of job.clients) {
        const state = session.clients.get(client)
        if (state) {
          state.waiting = []
          state.waitingBytes = 0
          state.behindFrom = null
        }
      }
    },
    uncut: () => {
      job.cut = -1
    },
    done: (parts, size) => sendSnapshot(context, session, job, parts, size),
  })
}

function sendSnapshot(
  context: SessionContext,
  session: Session,
  job: SnapshotJob,
  parts: Buffer[],
  size: TerminalSize,
): void {
  if (session.snapshot !== job) {
    return
  }
  // Over several turns. The clients stay in the snapshot phase meanwhile, so new output waits in
  // their `waiting` lists, and a client that leaves is no longer in the job.
  job.sending = true
  flushOutput(context, session)
  for (const client of job.clients) {
    client.send({ type: 'snapshot', cols: size.cols, rows: size.rows })
  }
  const frames = parts.flatMap((part) => splitFrames(part))
  const sendTurn = () => {
    if (session.snapshot !== job) {
      return
    }
    const began = performance.now()
    let sent = 0
    while (sent < SNAPSHOT_SEND_BYTES && performance.now() - began < SERIALIZE_SLICE_MS) {
      const frame = frames.shift()
      if (frame === undefined) {
        break
      }
      sent += frame.length
      for (const client of job.clients) {
        client.send(frame)
      }
    }
    if (frames.length > 0) {
      setImmediate(sendTurn)
    } else {
      endSnapshot(context, session, job)
    }
  }
  sendTurn()
}

function endSnapshot(context: SessionContext, session: Session, job: SnapshotJob): void {
  session.snapshot = undefined
  // Output pending now goes to the waiting lists, so replayed covers it.
  flushOutput(context, session)
  for (const client of job.clients) {
    const state = session.clients.get(client)
    if (!state) {
      continue
    }
    for (const frame of state.waiting ?? []) {
      client.send(frame)
    }
    const offset = state.behindFrom ?? session.offset
    client.send({ type: 'replayed', offset })
    state.waiting = null
    state.waitingBytes = 0
    // A catch-up does not count as unacked: the client acks its end once it is parsed.
    state.sent = offset
    state.acked = offset
    state.ackedAt = context.now()
    state.phase = state.behindFrom === null ? 'live' : 'behind'
  }
  job.finish()
  if (session.nextSnapshot.size > 0) {
    const next = new Set(session.nextSnapshot)
    session.nextSnapshot.clear()
    startSnapshot(context, session, next)
  }
  catchUpBehind(context, session)
}

// Catches up the clients that are behind once they can take it (spec 4.3): they acked what they
// got, they are visible, and the shell is quiet or they waited RESYNC_MAX_WAIT_MS. From the ring
// when it still has what they missed, else with a snapshot. Otherwise it looks again at the
// earliest time one of them could be ready.
// A snapshot for a shell that still floods comes at most once per RESYNC_MAX_WAIT_MS, so it
// cannot loop. A quiet shell cannot loop, so its snapshot goes at once: the prompt after a
// Ctrl+C shows without waiting.
export function catchUpBehind(context: SessionContext, session: Session): void {
  clearTimeout(session.resyncTimer)
  session.resyncTimer = undefined
  const now = context.now()
  let next = Number.POSITIVE_INFINITY
  for (const [client, state] of session.clients) {
    if (state.phase !== 'behind' || state.behindFrom === null || !state.visible) {
      continue
    }
    const quietAt = quietAtOf(session, now)
    // The dashboard acks every ACK_EVERY_BYTES, so a smaller rest is all it can have parsed.
    if (state.sent - state.acked >= ACK_EVERY_BYTES) {
      // Its acks stopped: its offset cannot be trusted, so only a snapshot fixes it.
      const stalled = Math.max(
        quietAt,
        state.ackedAt + ACK_STALL_MS,
        state.snapshotAt + RESYNC_MAX_WAIT_MS,
      )
      if (stalled > now) {
        next = Math.min(next, stalled)
      } else {
        snapshotCatchUp(context, session, client, state, now)
      }
      continue
    }
    const ready = Math.min(state.behindSince + RESYNC_MAX_WAIT_MS, quietAt)
    if (ready > now) {
      next = Math.min(next, ready)
      continue
    }
    flushOutput(context, session)
    const missed = ringSince(session, state.behindFrom)
    if (missed) {
      client.send({ type: 'resume', offset: state.behindFrom })
      for (const frame of missed) {
        client.send(frame)
      }
      client.send({ type: 'replayed', offset: session.offset })
      state.sent = session.offset
      state.acked = session.offset
      state.ackedAt = now
      state.behindFrom = null
      state.phase = 'live'
    } else if (quietAt <= now || now >= state.snapshotAt + RESYNC_MAX_WAIT_MS) {
      snapshotCatchUp(context, session, client, state, now)
    } else {
      next = Math.min(next, quietAt, state.snapshotAt + RESYNC_MAX_WAIT_MS)
    }
  }
  if (next < Number.POSITIVE_INFINITY) {
    session.resyncTimer = setTimeout(() => catchUpBehind(context, session), next - now)
  }
}

function snapshotCatchUp(
  context: SessionContext,
  session: Session,
  client: TerminalClient,
  state: ClientState,
  now: number,
): void {
  state.behindFrom = null
  state.phase = 'snapshot'
  state.snapshotAt = now
  requestSnapshot(context, session, client, state)
}

// When the shell counts as quiet if it prints nothing more: less than cols * rows / 8 bytes in
// the last RESYNC_QUIET_WINDOW_MS.
function quietAtOf(session: Session, now: number): number {
  dropOldOutput(session, now)
  const limit = (session.size.cols * session.size.rows) / 8
  let left = session.recentBytes
  let at = now
  for (const entry of session.recent) {
    if (left < limit) {
      break
    }
    left -= entry.bytes
    at = entry.at + RESYNC_QUIET_WINDOW_MS
  }
  return at
}

export function endSnapshots(session: Session): void {
  session.snapshot?.finish()
  session.snapshot = undefined
  session.nextSnapshot.clear()
}
