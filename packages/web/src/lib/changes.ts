import type { ChangeStatus } from '@helmsman/core'

export const CHANGE_LABEL: Record<ChangeStatus, string> = {
  M: 'Modified',
  A: 'Added',
  D: 'Deleted',
  R: 'Renamed',
  U: 'New file',
  C: 'Conflict',
}

// "src/lib/a.ts" becomes the folder "src/lib/" and the name "a.ts", the way VS Code lists a change.
export function splitPath(path: string): { folder: string; name: string } {
  const slash = path.lastIndexOf('/')
  return slash === -1
    ? { folder: '', name: path }
    : { folder: path.slice(0, slash + 1), name: path.slice(slash + 1) }
}

export function countLabel(count: number): string {
  return count === 1 ? '1 changed file' : `${count} changed files`
}

// A file that is new or gone is not a diff. It is shown whole, as all added or all removed.
export function wholeFileKind(status: ChangeStatus): 'added' | 'removed' | null {
  if (status === 'A' || status === 'U') {
    return 'added'
  }
  return status === 'D' ? 'removed' : null
}

// A file that ends with a line break would show an empty last line in an editor.
export function trimTrailingNewline(text: string): string {
  return text.replace(/\r?\n$/, '')
}

export function countLines(text: string): number {
  const trimmed = trimTrailingNewline(text)
  return trimmed === '' ? 0 : trimmed.split('\n').length
}

// The shape of Monaco's line changes, so this file does not import the editor.
export type LineChange = {
  originalStartLineNumber: number
  originalEndLineNumber: number
  modifiedStartLineNumber: number
  modifiedEndLineNumber: number
}

// Monaco gives an end line of 0 when a change has no lines on that side.
export function countLineChanges(changes: readonly LineChange[]): {
  added: number
  removed: number
} {
  let added = 0
  let removed = 0
  for (const change of changes) {
    if (change.modifiedEndLineNumber > 0) {
      added += change.modifiedEndLineNumber - change.modifiedStartLineNumber + 1
    }
    if (change.originalEndLineNumber > 0) {
      removed += change.originalEndLineNumber - change.originalStartLineNumber + 1
    }
  }
  return { added, removed }
}

// The file before or after the current one. It stops at the ends, it does not wrap.
export function stepFile(paths: readonly string[], current: string | null, step: 1 | -1) {
  const index = current === null ? -1 : paths.indexOf(current)
  const next = Math.min(Math.max(index + step, 0), paths.length - 1)
  return paths[next] ?? null
}

// Prose has long lines that are better wrapped. Code is not.
export function wrapsByDefault(path: string): boolean {
  const name = path.slice(path.lastIndexOf('/') + 1)
  const dot = name.lastIndexOf('.')
  if (dot <= 0) {
    return true
  }
  return ['md', 'markdown', 'txt', 'rst'].includes(name.slice(dot + 1).toLowerCase())
}
