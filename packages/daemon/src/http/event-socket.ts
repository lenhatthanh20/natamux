import type { FastifyInstance } from 'fastify'
import type { DaemonMessage } from '../shared/event-bus.js'
import type { RouteDeps } from './deps.js'

// An /events socket with more than this not sent yet skips messages, and gets `resync` once it
// is below again, every DRAIN_CHECK_MS.
export const EVENTS_HIGH_WATER = 1024 * 1024
export const DRAIN_CHECK_MS = 250
const RESYNC = JSON.stringify({ kind: 'resync' } satisfies DaemonMessage)
const SOCKET_OPEN = 1

// The part of a WebSocket that /events uses.
export type EventSocket = {
  readonly bufferedAmount: number
  readonly readyState: number
  send: (text: string) => void
}

type EventSender = { send: (text: string) => void; stop: () => void }

// Sends the messages of the bus to one /events socket. A dashboard that is too far behind (a
// sleeping laptop, a slow link) does not make the daemon hold every message for it.
export function eventSender(socket: EventSocket): EventSender {
  let drainTimer: NodeJS.Timeout | undefined
  const stop = () => {
    clearInterval(drainTimer)
    drainTimer = undefined
  }
  const send = (text: string) => {
    if (socket.readyState !== SOCKET_OPEN || drainTimer !== undefined) {
      return
    }
    if (socket.bufferedAmount <= EVENTS_HIGH_WATER) {
      socket.send(text)
      return
    }
    drainTimer = setInterval(() => {
      if (socket.readyState !== SOCKET_OPEN) {
        stop()
      } else if (socket.bufferedAmount <= EVENTS_HIGH_WATER) {
        stop()
        socket.send(RESYNC)
      }
    }, DRAIN_CHECK_MS)
    drainTimer.unref()
  }
  return { send, stop }
}

export function registerEventRoutes(app: FastifyInstance, deps: RouteDeps): void {
  const { options } = deps

  app.get('/events', { websocket: true }, (socket) => {
    const sender = eventSender(socket)
    const unsubscribe = options.bus.subscribeClient(sender.send)
    // The pull request statuses may be old: nobody looked while no dashboard listened.
    if (options.bus.size() === 1) {
      options.pullRequests.wake()
    }
    const end = () => {
      unsubscribe()
      sender.stop()
    }
    socket.on('close', end)
    socket.on('error', end)
  })
}
