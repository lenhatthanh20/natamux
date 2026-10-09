import { useState } from 'react'

// A true/false choice kept per browser, like whether a section is open.
export function useStoredFlag(key: string, initial: boolean) {
  const [value, setValue] = useState(() => {
    try {
      const saved = globalThis.localStorage?.getItem(key)
      return saved === null || saved === undefined ? initial : saved === 'true'
    } catch {
      return initial
    }
  })
  const update = (next: boolean) => {
    setValue(next)
    try {
      globalThis.localStorage?.setItem(key, String(next))
    } catch {
      // Not kept; the next visit starts with the default.
    }
  }
  return [value, update] as const
}
