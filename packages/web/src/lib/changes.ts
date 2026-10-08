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
