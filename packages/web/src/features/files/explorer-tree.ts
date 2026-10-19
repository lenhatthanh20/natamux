import type { ChangeEntry, ChangeStatus, TreeEntry } from '@natamux/core'

// One row of the files explorer. The key is the path in the worktree. The shape fits TreeRow in
// lib/roving.ts.
export type ExplorerRow = {
  key: string
  name: string
  kind: TreeEntry['kind']
  parent: string | null
  // 1-based, like aria-level.
  level: number
  posInSet: number
  setSize: number
  expandable: boolean
  expanded: boolean
}

export function joinPath(folder: string, name: string): string {
  return folder === '' ? name : `${folder}/${name}`
}

// The rows on screen: the root, then the children of each open folder that was read. '' is the
// root.
export function explorerRows(
  folders: ReadonlyMap<string, readonly TreeEntry[]>,
  expanded: ReadonlySet<string>,
): ExplorerRow[] {
  const rows: ExplorerRow[] = []
  const walk = (folder: string, level: number) => {
    const entries = folders.get(folder) ?? []
    entries.forEach((entry, index) => {
      const key = joinPath(folder, entry.name)
      const open = entry.kind === 'folder' && expanded.has(key)
      rows.push({
        key,
        name: entry.name,
        kind: entry.kind,
        parent: folder === '' ? null : folder,
        level,
        posInSet: index + 1,
        setSize: entries.length,
        expandable: entry.kind === 'folder',
        expanded: open,
      })
      if (open) {
        walk(key, level + 1)
      }
    })
  }
  walk('', 1)
  return rows
}

// The status of each changed file, and the folders that hold one at any depth.
export function changeMarks(changes: readonly ChangeEntry[]): {
  files: Map<string, ChangeStatus>
  folders: Set<string>
} {
  const files = new Map<string, ChangeStatus>()
  const folders = new Set<string>()
  for (const change of changes) {
    files.set(change.path, change.status)
    const parts = change.path.split('/')
    for (let end = 1; end < parts.length; end += 1) {
      folders.add(parts.slice(0, end).join('/'))
    }
  }
  return { files, folders }
}

// Characters a shell reads as they are. Any other one gets the path quoted.
const PLAIN_PATH = /^[A-Za-z0-9._/@+-]+$/

// What a drop on a terminal types: the path from the worktree root, then a space, so the next
// word can follow. No Enter: the user finishes the prompt.
export function dropText(path: string, kind: TreeEntry['kind']): string {
  const text = kind === 'folder' ? `${path}/` : path
  return `${PLAIN_PATH.test(text) ? text : `'${text.replaceAll("'", "'\\''")}'`} `
}

// The data of a drag can only be read on the drop, but its types are seen on the way. The task is
// in the type, so a terminal of another task refuses the drag before it ends.
export function pathDragType(taskId: string): string {
  return `application/x-natamux-path-${taskId.toLowerCase()}`
}
