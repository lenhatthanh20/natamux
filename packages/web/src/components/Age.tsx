import { useNow } from '../hooks/useNow'
import { type AgeStyle, formatAge } from '../lib/tasks'

// A relative time such as "5m". It reads the clock itself, so a tick re-renders only this text.
export function Age({ at, style }: { at: string; style?: AgeStyle }) {
  return formatAge(at, useNow(), style)
}
