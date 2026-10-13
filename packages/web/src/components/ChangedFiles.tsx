import type { ChangeEntry, ChangeStatus } from '@natamux/core'
import {
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  List,
  ListTree,
  RefreshCw,
} from 'lucide-react'
import {
  type FocusEvent,
  type KeyboardEvent,
  memo,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import type { FileViewMode } from '../hooks/useFileViewMode'
import { findRoving, rovingItem, useRovingFocus } from '../hooks/useRovingFocus'
import { useStableCallback } from '../hooks/useStableCallback'
import {
  allFolderKeys,
  ancestorKeys,
  buildChangeTree,
  entryCounts,
  flattenTree,
  listRows,
  type TreeRow,
  totalCounts,
} from '../lib/change-tree'
import { CHANGE_LABEL, countLabel, splitPath } from '../lib/changes'
import { shortcutOf } from '../lib/commands'
import { firstChildKey, parentKey } from '../lib/roving'
import { FileIcon } from './FileIcon'
import { IconButton } from './ui/IconButton'
import { ICON } from './ui/styles'

const STATUS_TONE: Record<ChangeStatus, string> = {
  M: 'text-modified',
  A: 'text-add',
  U: 'text-add',
  D: 'text-remove',
  C: 'text-remove',
  R: 'text-renamed',
}

// The folders closed in each task. Kept while the page lives, so they stay closed when the user
// goes to another tab and back.
const closedByTask = new Map<string, ReadonlySet<string>>()
const NONE: ReadonlySet<string> = new Set()

export type ChangedFilesProps = {
  taskId: string
  list: readonly ChangeEntry[]
  // The first read is done.
  loaded: boolean
  mode: FileViewMode
  onModeChange: (mode: FileViewMode) => void
  selectedPath: string | null
  onSelect: (path: string) => void
  onRefresh: () => void
  // An error or a skeleton, between the header and the rows.
  status?: ReactNode
}

// The header and the rows of the changed files, as a flat list or as folders.
export function ChangedFiles({
  taskId,
  list,
  loaded,
  mode,
  onModeChange,
  selectedPath,
  onSelect,
  onRefresh,
  status: notice,
}: ChangedFilesProps) {
  const [closed, setClosed] = useState(() => ({ taskId, keys: closedByTask.get(taskId) ?? NONE }))
  const collapsed = closed.taskId === taskId ? closed.keys : (closedByTask.get(taskId) ?? NONE)
  const setCollapsed = (keys: ReadonlySet<string>) => {
    closedByTask.set(taskId, keys)
    setClosed({ taskId, keys })
  }

  const tree = useMemo(() => buildChangeTree(list), [list])
  const rows = useMemo(
    () => (mode === 'tree' ? flattenTree(tree, collapsed) : listRows(list)),
    [mode, tree, collapsed, list],
  )
  const folderKeys = useMemo(() => allFolderKeys(tree), [tree])
  const total = useMemo(() => totalCounts(list), [list])
  const anyOpen = folderKeys.some((key) => !collapsed.has(key))
  const selectedKey = selectedPath === null ? null : `f:${selectedPath}`
  const byKey = useMemo(() => new Map<string, TreeRow>(rows.map((row) => [row.key, row])), [rows])
  // One array per row, kept while the rows stay, so a memoized row does not see a new prop.
  const ancestors = useMemo(() => {
    const found = new Map<string, TreeRow[]>()
    const of = (row: TreeRow): TreeRow[] => {
      const known = found.get(row.key)
      if (known !== undefined) {
        return known
      }
      const parent = row.parent === null ? undefined : byKey.get(row.parent)
      const chain = parent === undefined ? [] : [...of(parent), parent]
      found.set(row.key, chain)
      return chain
    }
    for (const row of rows) {
      of(row)
    }
    return found
  }, [rows, byKey])

  const treeRef = useRef<HTMLDivElement | null>(null)
  // The row that last had focus in the tree, and where it was.
  const lastFocus = useRef<{ key: string; index: number } | null>(null)
  const scrollTo = useRef<string | null>(null)

  const toggle = useStableCallback((key: string) => {
    const next = new Set(collapsed)
    if (!next.delete(key)) {
      next.add(key)
    }
    setCollapsed(next)
  })
  const select = useStableCallback((path: string) => onSelect(path))
  const remember = useStableCallback((key: string, index: number) => {
    lastFocus.current = { key, index }
  })

  const roving = useRovingFocus({
    keys: rows.map((row) => row.key),
    labels: rows.map((row) => row.node.name),
    preferred: selectedKey,
    onKey: (event, key) => {
      const row = byKey.get(key)
      if (row === undefined) {
        return false
      }
      if (event.key === 'ArrowRight') {
        if (!row.expandable) {
          return false
        }
        if (!row.expanded) {
          toggle(key)
        } else {
          const child = firstChildKey(rows, key)
          if (child !== null) {
            roving.focusKey(child)
          }
        }
        return true
      }
      if (event.key === 'ArrowLeft') {
        if (row.expandable && row.expanded) {
          toggle(key)
          return true
        }
        const parent = parentKey(rows, key)
        if (parent !== null) {
          roving.focusKey(parent)
          return true
        }
        return false
      }
      if (event.key === 'Enter' || event.key === ' ') {
        if (row.node.kind === 'file') {
          onSelect(row.node.key)
        } else {
          toggle(key)
        }
        return true
      }
      return false
    },
  })

  // A new selection opens the folders around it. Only a new one does: after Collapse all, the
  // selected file stays hidden until the user picks another.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above.
  useEffect(() => {
    if (selectedPath === null) {
      return
    }
    scrollTo.current = selectedPath
    if (mode !== 'tree') {
      return
    }
    const hidden = ancestorKeys(tree, selectedPath).filter((key) => collapsed.has(key))
    if (hidden.length > 0) {
      const next = new Set(collapsed)
      for (const key of hidden) {
        next.delete(key)
      }
      setCollapsed(next)
    }
  }, [selectedPath, mode])

  // After every render: the selected row exists only once its folders are open.
  useEffect(() => {
    const path = scrollTo.current
    const row = path === null ? null : findRoving(treeRef.current, `f:${path}`)
    if (row !== null) {
      scrollTo.current = null
      row.scrollIntoView?.({ block: 'nearest' })
    }
  })

  // A refresh can take away the row that had focus, and the focus with it. The row now in its
  // place takes the focus, so the keyboard stays in the list.
  useEffect(() => {
    const last = lastFocus.current
    if (last === null || rows.some((row) => row.key === last.key)) {
      return
    }
    const active = document.activeElement
    if (active !== null && active !== document.body) {
      return
    }
    const target = rows[Math.min(last.index, rows.length - 1)]
    if (target !== undefined) {
      findRoving(treeRef.current, target.key)?.focus()
    }
  }, [rows])

  const treeEvents = {
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
      // [ and ] step through the files from anywhere, so type-ahead must not take them.
      if (event.key === '[' || event.key === ']') {
        return
      }
      roving.containerProps.onKeyDown(event)
    },
    onBlur: (event: FocusEvent<HTMLDivElement>) => {
      const next = event.relatedTarget
      const left = event.target
      if (next instanceof Node) {
        if (!event.currentTarget.contains(next)) {
          lastFocus.current = null
        }
        return
      }
      // Focus went nowhere: either the user clicked a blank part of the page, or a refresh took
      // the row away. Only a row still in the page means the user left.
      queueMicrotask(() => {
        if (left.isConnected) {
          lastFocus.current = null
        }
      })
    },
  }

  return (
    <>
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-line pr-1 pl-3">
        <span className="min-w-0 truncate text-xs font-medium whitespace-nowrap text-muted">
          {loaded ? countLabel(list.length) : 'Reading changes.'}
        </span>
        {total !== null && (
          <span
            data-tip={`${total.added} lines added, ${total.removed} removed`}
            className="shrink-0 font-mono text-xs whitespace-nowrap tabular-nums"
          >
            <span className="text-add">+{total.added}</span>{' '}
            <span className="text-remove">−{total.removed}</span>
          </span>
        )}
        <span className="flex-1" />
        <div className="flex shrink-0 items-center gap-0.5">
          {mode === 'tree' && (
            <IconButton
              label={anyOpen ? 'Collapse all folders' : 'Expand all folders'}
              icon={anyOpen ? ChevronsDownUp : ChevronsUpDown}
              size="xs"
              disabled={folderKeys.length === 0}
              onClick={() => setCollapsed(anyOpen ? new Set(folderKeys) : NONE)}
            />
          )}
          <IconButton
            label="View as tree"
            icon={mode === 'tree' ? List : ListTree}
            size="xs"
            aria-pressed={mode === 'tree'}
            onClick={() => onModeChange(mode === 'tree' ? 'list' : 'tree')}
          />
          <IconButton
            label="Refresh"
            icon={RefreshCw}
            size="xs"
            shortcut={shortcutOf('changes.refresh')}
            onClick={onRefresh}
          />
        </div>
      </div>
      {notice}
      <div
        ref={(node) => {
          treeRef.current = node
          roving.container(node)
        }}
        role="tree"
        aria-label="Changed files"
        aria-multiselectable="false"
        {...treeEvents}
        className="group/tree @container min-h-0 flex-1 overflow-y-auto p-1"
      >
        {rows.map((row, index) => (
          <ChangedRow
            key={row.key}
            row={row}
            index={index}
            mode={mode}
            selected={row.key === selectedKey}
            isStop={row.key === roving.stop}
            ancestors={ancestors.get(row.key) ?? NO_ANCESTORS}
            activate={roving.activate}
            remember={remember}
            onSelect={select}
            onToggle={toggle}
          />
        ))}
      </div>
    </>
  )
}

const NO_ANCESTORS: TreeRow[] = []

type ChangedRowProps = {
  row: TreeRow
  index: number
  mode: FileViewMode
  selected: boolean
  isStop: boolean
  ancestors: readonly TreeRow[]
  activate: (key: string) => void
  remember: (key: string, index: number) => void
  onSelect: (path: string) => void
  onToggle: (key: string) => void
}

// Memoized: all its props are values or stable functions, so a focus move or a new selection
// renders only the rows that changed. A new list builds new rows, so rows are compared by what
// they show (sameRowProps). content-visibility skips the rows far off screen.
const ChangedRow = memo(function ChangedRow({
  row,
  index,
  mode,
  selected,
  isStop,
  ancestors,
  activate,
  remember,
  onSelect,
  onToggle,
}: ChangedRowProps) {
  const tab = rovingItem(row.key, isStop, activate)
  const node = row.node
  const isFile = node.kind === 'file'
  return (
    // The tree handles the keys of every row, see treeEvents.
    // biome-ignore lint/a11y/useKeyWithClickEvents: see above.
    <div
      role="treeitem"
      {...tab}
      tabIndex={tab.tabIndex}
      onFocus={(event) => {
        tab.onFocus(event)
        if (event.target === event.currentTarget) {
          remember(row.key, index)
        }
      }}
      onClick={() => (isFile ? onSelect(node.key) : onToggle(row.key))}
      aria-level={row.level}
      aria-posinset={row.posInSet}
      aria-setsize={row.setSize}
      aria-selected={isFile ? selected : undefined}
      aria-expanded={row.expandable ? row.expanded : undefined}
      aria-label={node.kind === 'file' ? fileLabel(node.entry) : folderLabel(node)}
      style={{
        paddingLeft: mode === 'tree' ? `calc(0.25rem + ${row.level - 1} * 0.75rem)` : '0.5rem',
      }}
      className="cv-row focus-inset relative flex h-(--row-h) cursor-pointer items-center gap-1.5 rounded-md pr-2 select-none hover:bg-hover aria-selected:bg-selected group-focus-within/tree:aria-selected:bg-selection-active"
    >
      {ancestors.map((ancestor) => (
        <span
          key={ancestor.key}
          aria-hidden="true"
          style={{ left: `calc(0.75rem + ${ancestor.level - 1} * 0.75rem)` }}
          className="pointer-events-none absolute inset-y-0 w-px bg-line opacity-0 group-hover/tree:opacity-100 group-focus-within/tree:opacity-100 motion-safe:transition-opacity"
        />
      ))}
      {mode === 'tree' && (
        <span
          aria-hidden="true"
          className="flex size-4 shrink-0 items-center justify-center text-muted"
        >
          {node.kind === 'file' ? (
            <FileIcon path={node.key} />
          ) : (
            <ChevronRight
              size={ICON.xs}
              className={`motion-safe:transition-transform ${row.expanded ? 'rotate-90' : ''}`}
            />
          )}
        </span>
      )}
      {mode === 'list' && node.kind === 'file' && <FileIcon path={node.key} />}
      {node.kind === 'file' ? (
        <FileContent entry={node.entry} name={node.name} showFolder={mode === 'list'} />
      ) : (
        <>
          <span data-tip={node.key} className="min-w-0 truncate text-ink-2">
            {node.name}
          </span>
          <span className="flex-1" />
          {!row.expanded && node.added !== null && (
            <Counts added={node.added} removed={node.removed ?? 0} />
          )}
          <span className="shrink-0 text-xs text-faint tabular-nums">{node.fileCount}</span>
          <span
            aria-hidden="true"
            className={`flex size-4 shrink-0 items-center justify-center ${STATUS_TONE[node.status]}`}
          >
            <span className="size-1.5 rounded-full bg-current" />
          </span>
        </>
      )}
    </div>
  )
}, sameRowProps)

function sameRowProps(before: ChangedRowProps, after: ChangedRowProps): boolean {
  return (
    sameRow(before.row, after.row) &&
    before.index === after.index &&
    before.mode === after.mode &&
    before.selected === after.selected &&
    before.isStop === after.isStop &&
    before.ancestors.length === after.ancestors.length &&
    // A guide line shows only the key and level of each folder above.
    before.ancestors.every(
      (row, at) => row.key === after.ancestors[at]?.key && row.level === after.ancestors[at]?.level,
    ) &&
    before.activate === after.activate &&
    before.remember === after.remember &&
    before.onSelect === after.onSelect &&
    before.onToggle === after.onToggle
  )
}

function sameRow(before: TreeRow, after: TreeRow): boolean {
  if (before === after) {
    return true
  }
  const one = before.node
  const two = after.node
  const sameNode =
    one.kind === 'file' && two.kind === 'file'
      ? sameEntry(one.entry, two.entry)
      : one.kind === 'folder' &&
        two.kind === 'folder' &&
        one.fileCount === two.fileCount &&
        one.added === two.added &&
        one.removed === two.removed &&
        one.status === two.status
  return (
    sameNode &&
    one.key === two.key &&
    one.name === two.name &&
    before.key === after.key &&
    before.level === after.level &&
    before.posInSet === after.posInSet &&
    before.setSize === after.setSize &&
    before.expandable === after.expandable &&
    before.expanded === after.expanded
  )
}

function sameEntry(before: ChangeEntry, after: ChangeEntry): boolean {
  const keys = Object.keys(after) as (keyof ChangeEntry)[]
  return (
    keys.length === Object.keys(before).length && keys.every((key) => before[key] === after[key])
  )
}

function fileLabel(entry: ChangeEntry): string {
  const { folder, name } = splitPath(entry.path)
  const place = folder === '' ? 'top folder' : folder.slice(0, -1)
  let label = `${name}, ${CHANGE_LABEL[entry.status]}, ${place}`
  const counts = entryCounts(entry)
  if (counts !== null) {
    label += `, ${counts.added} added, ${counts.removed} removed`
  }
  if (entry.status === 'R' && entry.originalPath !== null) {
    label += `, renamed from ${entry.originalPath}`
  }
  return label
}

function folderLabel(folder: { name: string; fileCount: number }): string {
  return `${folder.name}, ${countLabel(folder.fileCount)}`
}

// Hidden when the panel is narrow: the name matters more.
function Counts({ added, removed }: { added: number; removed: number }) {
  return (
    <span className="hidden shrink-0 font-mono text-2xs whitespace-nowrap tabular-nums @min-[15rem]:inline">
      <span className="text-add">+{added}</span> <span className="text-remove">−{removed}</span>
    </span>
  )
}

function FileContent({
  entry,
  name,
  showFolder,
}: {
  entry: ChangeEntry
  name: string
  showFolder: boolean
}) {
  const { folder } = splitPath(entry.path)
  const hint = showFolder && folder !== ''
  const counts = entryCounts(entry)
  const tone = STATUS_TONE[entry.status]
  const tip =
    entry.status === 'R' && entry.originalPath !== null
      ? `${entry.originalPath} → ${entry.path}`
      : entry.path
  const strike = entry.status === 'D' ? 'line-through' : ''
  // With a folder hint the name keeps its width and the folder is what shortens.
  const width = hint ? 'max-w-[75%] shrink-0' : 'min-w-0'
  return (
    <>
      <span data-tip={tip} className={`truncate font-medium ${tone} ${strike} ${width}`}>
        {name}
      </span>
      {hint ? (
        // The folder loses its start when it is long: the end names the place.
        <span
          className="min-w-0 flex-1 truncate text-xs text-muted"
          style={{ direction: 'rtl', textAlign: 'left' }}
        >
          {`${folder}\u200e`}
        </span>
      ) : (
        <span className="flex-1" />
      )}
      {counts !== null && <Counts added={counts.added} removed={counts.removed} />}
      <span
        data-tip={CHANGE_LABEL[entry.status]}
        className={`flex size-4 shrink-0 items-center justify-center font-mono text-xs font-semibold ${tone}`}
      >
        {entry.status}
      </span>
    </>
  )
}
