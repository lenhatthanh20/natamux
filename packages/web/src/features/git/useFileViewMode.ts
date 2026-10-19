import { useState } from 'react'

export type FileViewMode = 'list' | 'tree'

const KEY = 'natamux.changes-view'

function read(): FileViewMode {
  try {
    return globalThis.localStorage?.getItem(KEY) === 'tree' ? 'tree' : 'list'
  } catch {
    return 'list'
  }
}

// How the Changes tab shows its files, kept per browser.
export function useFileViewMode(): readonly [FileViewMode, (mode: FileViewMode) => void] {
  const [mode, setMode] = useState<FileViewMode>(read)
  const update = (next: FileViewMode) => {
    setMode(next)
    try {
      globalThis.localStorage?.setItem(KEY, next)
    } catch {
      // Not kept; the next visit starts with the list.
    }
  }
  return [mode, update] as const
}
