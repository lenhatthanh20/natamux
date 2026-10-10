import type { ChangeEntry, ChangeStatus } from '@natamux/core'
import type { FileViewMode } from '../hooks/useFileViewMode'

// Older daemons send no line counts, and null means the daemon could not count (binary, too big).
type Counted = { added?: number | null; removed?: number | null }

export type LineCounts = { added: number; removed: number }

export type TreeFolder = {
  kind: 'folder'
  // The full path of the deepest folder in the row, like 'packages/web/src'.
  key: string
  // The label, relative to the parent. Compacted folders join with '/', like 'web/src'.
  name: string
  depth: number
  children: TreeNode[]
  // Files below, at any depth.
  fileCount: number
  added: number | null
  removed: number | null
  // The strongest status below, so a folder shows what needs the most care.
  status: ChangeStatus
}

export type TreeFile = {
  kind: 'file'
  key: string
  name: string
  depth: number
  entry: ChangeEntry
}

export type TreeNode = TreeFolder | TreeFile

// One row on screen. The shape fits TreeRow in lib/roving.ts.
export type TreeRow = {
  // 'd:' and a folder key, or 'f:' and a path.
  key: string
  node: TreeNode
  parent: string | null
  // 1-based, like aria-level.
  level: number
  posInSet: number
  setSize: number
  expandable: boolean
  expanded: boolean
}

export function entryCounts(entry: ChangeEntry): LineCounts | null {
  const { added, removed } = entry as ChangeEntry & Counted
  if (typeof added !== 'number') {
    return null
  }
  return { added, removed: typeof removed === 'number' ? removed : 0 }
}

// The sum over the entries that have counts, or null when none has.
export function totalCounts(list: readonly ChangeEntry[]): LineCounts | null {
  let counted = false
  let added = 0
  let removed = 0
  for (const entry of list) {
    const counts = entryCounts(entry)
    if (counts !== null) {
      counted = true
      added += counts.added
      removed += counts.removed
    }
  }
  return counted ? { added, removed } : null
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

// Natural order ('file2' before 'file10'), case-insensitive. Names the collator finds equal, like
// 'a.ts' and 'A.ts', fall back to code points so the order never flips between reads.
export function compareNames(a: string, b: string): number {
  return collator.compare(a, b) || (a < b ? -1 : a > b ? 1 : 0)
}

const STATUS_RANK: Record<ChangeStatus, number> = { C: 6, D: 5, M: 4, R: 3, A: 2, U: 1 }

function stronger(a: ChangeStatus, b: ChangeStatus): ChangeStatus {
  return STATUS_RANK[b] > STATUS_RANK[a] ? b : a
}

function addCount(sum: number | null, value: number | null | undefined): number | null {
  return typeof value === 'number' ? (sum ?? 0) + value : sum
}

export function rowKey(node: TreeNode): string {
  return node.kind === 'folder' ? `d:${node.key}` : `f:${node.key}`
}

type Dir = { name: string; path: string; dirs: Map<string, Dir>; files: ChangeEntry[] }

function newDir(name: string, path: string): Dir {
  return { name, path, dirs: new Map(), files: [] }
}

function nodesOf(dir: Dir, depth: number, compact: boolean): TreeNode[] {
  const folders = [...dir.dirs.values()]
    .sort((a, b) => compareNames(a.name, b.name))
    .map((child) => folderOf(child, depth, compact))
  const files = dir.files
    .map((entry) => fileNode(entry, depth))
    .sort((a, b) => compareNames(a.name, b.name))
  return [...folders, ...files]
}

function fileNode(entry: ChangeEntry, depth: number): TreeFile {
  const name = entry.path.slice(entry.path.lastIndexOf('/') + 1)
  return { kind: 'file', key: entry.path, name, depth, entry }
}

function folderOf(start: Dir, depth: number, compact: boolean): TreeFolder {
  let dir = start
  const names = [start.name]
  // A folder that holds only one folder adds a row that says nothing, so the two share a row.
  while (compact && dir.files.length === 0 && dir.dirs.size === 1) {
    const only = [...dir.dirs.values()][0]
    if (only === undefined) {
      break
    }
    dir = only
    names.push(only.name)
  }
  const children = nodesOf(dir, depth + 1, compact)
  let fileCount = 0
  let added: number | null = null
  let removed: number | null = null
  let status: ChangeStatus = 'U'
  for (const child of children) {
    if (child.kind === 'folder') {
      fileCount += child.fileCount
      added = addCount(added, child.added)
      removed = addCount(removed, child.removed)
      status = stronger(status, child.status)
    } else {
      const counts = child.entry as ChangeEntry & Counted
      fileCount += 1
      added = addCount(added, counts.added)
      removed = addCount(removed, counts.removed)
      status = stronger(status, child.entry.status)
    }
  }
  return {
    kind: 'folder',
    key: dir.path,
    name: names.join('/'),
    depth,
    children,
    fileCount,
    added,
    removed,
    status,
  }
}

// The changed files as folders, like an editor's explorer. A renamed file sits at its new path.
export function buildChangeTree(
  list: readonly ChangeEntry[],
  { compact = true }: { compact?: boolean } = {},
): TreeNode[] {
  const root = newDir('', '')
  for (const entry of list) {
    let dir = root
    for (const part of entry.path.split('/').slice(0, -1)) {
      let next = dir.dirs.get(part)
      if (next === undefined) {
        next = newDir(part, dir.path === '' ? part : `${dir.path}/${part}`)
        dir.dirs.set(part, next)
      }
      dir = next
    }
    dir.files.push(entry)
  }
  return nodesOf(root, 0, compact)
}

// The rows on screen. `collapsed` holds row keys ('d:' and a folder key); their children are left
// out.
export function flattenTree(nodes: readonly TreeNode[], collapsed: ReadonlySet<string>): TreeRow[] {
  const rows: TreeRow[] = []
  const walk = (siblings: readonly TreeNode[], parent: string | null, level: number) => {
    for (const [index, node] of siblings.entries()) {
      const key = rowKey(node)
      const expanded = node.kind === 'folder' && !collapsed.has(key)
      rows.push({
        key,
        node,
        parent,
        level,
        posInSet: index + 1,
        setSize: siblings.length,
        expandable: node.kind === 'folder',
        expanded,
      })
      if (node.kind === 'folder' && expanded) {
        walk(node.children, key, level + 1)
      }
    }
  }
  walk(nodes, null, 1)
  return rows
}

// The rows of the flat list, in the order the daemon gave.
export function listRows(list: readonly ChangeEntry[]): TreeRow[] {
  return list.map((entry, index) => {
    const node = fileNode(entry, 0)
    return {
      key: rowKey(node),
      node,
      parent: null,
      level: 1,
      posInSet: index + 1,
      setSize: list.length,
      expandable: false,
      expanded: false,
    }
  })
}

// The row keys of the folders around a path, outermost first.
export function ancestorKeys(nodes: readonly TreeNode[], path: string): string[] {
  const folder = nodes.find(
    (node): node is TreeFolder => node.kind === 'folder' && path.startsWith(`${node.key}/`),
  )
  return folder === undefined ? [] : [rowKey(folder), ...ancestorKeys(folder.children, path)]
}

export function allFolderKeys(nodes: readonly TreeNode[]): string[] {
  const keys: string[] = []
  for (const node of nodes) {
    if (node.kind === 'folder') {
      keys.push(rowKey(node), ...allFolderKeys(node.children))
    }
  }
  return keys
}

function filePaths(nodes: readonly TreeNode[], into: string[]): string[] {
  for (const node of nodes) {
    if (node.kind === 'folder') {
      filePaths(node.children, into)
    } else {
      into.push(node.key)
    }
  }
  return into
}

// The paths in the order the panel shows them, for [ and ]. Collapsed folders count too: stepping
// into one opens it.
export function fileOrder(list: readonly ChangeEntry[], mode: FileViewMode): string[] {
  return mode === 'tree' ? filePaths(buildChangeTree(list), []) : list.map((entry) => entry.path)
}
