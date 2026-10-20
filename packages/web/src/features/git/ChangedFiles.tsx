import type { ChangeEntry } from '@natamux/core'
import { ChevronsDownUp, ChevronsUpDown, List, ListTree, RefreshCw, Undo2 } from 'lucide-react'
import {
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { IconButton } from '../../components/ui/IconButton'
import { openContextMenu } from '../../components/ui/Menu'
import { findRoving, useRovingFocus } from '../../hooks/useRovingFocus'
import { useStableCallback } from '../../hooks/useStableCallback'
import { countLabel } from '../../lib/changes'
import { shortcutOf } from '../../lib/commands'
import { firstChildKey, parentKey } from '../../lib/roving'
import { ChangedRow } from './ChangedRow'
import {
  allFolderKeys,
  ancestorKeys,
  buildChangeTree,
  entriesUnder,
  flattenTree,
  listRows,
  type TreeRow,
  totalCounts,
} from './change-tree'
import type { FileViewMode } from './useFileViewMode'

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
  // Asks to roll back these files. Without it, the list offers no rollback.
  onRollback?: (entries: readonly ChangeEntry[]) => void
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
  onRollback,
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

  // The menu of a row, under the row for a key or at the pointer for a click.
  const openRowMenu = (
    target: EventTarget,
    event: { preventDefault: () => void; clientX?: number; clientY?: number },
  ) => {
    const element = target instanceof Element ? target.closest<HTMLElement>('[data-roving]') : null
    const row = element === null ? undefined : byKey.get(element.dataset.roving ?? '')
    if (element === null || row === undefined || onRollback === undefined) {
      return
    }
    const entries = entriesUnder(row.node).filter((entry) => entry.status !== 'C')
    const opener = {
      preventDefault: () => event.preventDefault(),
      clientX: event.clientX,
      clientY: event.clientY,
      currentTarget: element,
    }
    openContextMenu(
      opener,
      [
        {
          label: 'Rollback…',
          icon: Undo2,
          disabled: entries.length === 0,
          hint: entries.length === 0 ? 'Resolve the conflict first' : undefined,
          onSelect: () => onRollback(entries),
        },
      ],
      `Actions for ${row.node.name}`,
    )
  }

  const treeEvents = {
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
      // [ and ] step through the files from anywhere, so type-ahead must not take them.
      if (event.key === '[' || event.key === ']') {
        return
      }
      if ((event.key === 'F10' && event.shiftKey) || event.key === 'ContextMenu') {
        event.stopPropagation()
        openRowMenu(event.target, event)
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
          {onRollback !== undefined && (
            <IconButton
              label="Rollback…"
              icon={Undo2}
              size="xs"
              disabled={list.length === 0}
              onClick={() => onRollback(list)}
            />
          )}
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
        onContextMenu={(event) => openRowMenu(event.target, event)}
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
