import { type Task, taskSchema } from '@helmsman/core'

export function liveSocketUrl(
  location: Pick<Location, 'protocol' | 'host'>,
  token: string,
): string {
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws'
  return `${scheme}://${location.host}/api/events?token=${encodeURIComponent(token)}`
}

// Returns the changed task, or undefined for a message this dashboard does not use.
export function parseLiveMessage(text: string): Task | undefined {
  let message: unknown
  try {
    message = JSON.parse(text)
  } catch {
    return undefined
  }
  if (typeof message !== 'object' || message === null || !('task' in message)) {
    return undefined
  }
  const parsed = taskSchema.safeParse(message.task)
  return parsed.success ? parsed.data : undefined
}

// WebSocket.CONNECTING. The number, so a stand-in socket in a test works too.
const CONNECTING = 0

// Closing a socket that is still connecting makes the browser log "closed before the connection
// is established". Such a socket is closed as soon as it opens, and its handlers are dropped so
// nothing reacts to it meanwhile.
export function closeSocket(socket: WebSocket): void {
  if (socket.readyState === CONNECTING) {
    socket.onmessage = null
    socket.onclose = null
    socket.onerror = null
    socket.onopen = () => socket.close()
    return
  }
  socket.close()
}
