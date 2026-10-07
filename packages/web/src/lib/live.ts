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
