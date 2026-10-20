import type { ChangeEntry, TreeEntry } from '@natamux/core'
import { useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { ChevronRight, RefreshCw } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { FileIcon } from '../../components/FileIcon'
import { IconButton } from '../../components/ui/IconButton'
import { SkeletonList } from '../../components/ui/Skeleton'
import { ICON } from '../../components/ui/styles'
import { useRovingFocus } from '../../hooks/useRovingFocus'
import { CHANGE_LABEL } from '../../lib/changes'
import { FALLBACK_POLL_MS } from '../../lib/live'
import { firstChildKey, parentKey } from '../../lib/roving'
import { STATUS_TONE } from '../git/ChangedRow'
import { changeMarks, type ExplorerRow, explorerRows, pathDragType } from './explorer-tree'
import { draftKey, useFileDrafts } from './file-drafts'

// The open folders of each task. Kept while the page lives, so they stay open when the details
// show another view and back.
const openByTask = new Map<string, ReadonlySet<string>>()
const NONE: ReadonlySet<string> = new Set()

type FileTreeProps = {
  taskId: string
  // One folder of the worktree; '' is the root.
  loadFolder: (path: string) => Promise<TreeEntry[]>
  loadChanges: () => Promise<ChangeEntry[]>
  // The file in the editor tab, if any.
  openPath: string | null
  onOpen: (path: string) => void
}

// The files of the task's worktree. A folder is read when it opens. A row dragged onto a terminal
// of the same task types its path there (see TerminalPane).
export function FileTree({ taskId, loadFolder, loadChanges, openPath, onOpen }: FileTreeProps) {
  const [open, setOpen] = useState(() => ({ taskId, keys: openByTask.get(taskId) ?? NONE }))
  const expanded = open.taskId === taskId ? open.keys : (openByTask.get(taskId) ?? NONE)
  const setExpanded = (keys: ReadonlySet<string>) => {
    openByTask.set(taskId, keys)
    setOpen({ taskId, keys })
  }
  const toggle = (key: string) => {
    const next = new Set(expanded)
    if (!next.delete(key)) {
      next.add(key)
    }
    setExpanded(next)
  }

  // Only folders on screen are read: an open folder inside a closed one waits.
  const shown = [
    '',
    ...[...expanded].filter((path) => {
      const parts = path.split('/')
      return parts.every((_, end) => expanded.has(parts.slice(0, end + 1).join('/')))
    }),
  ]
  // workspace.dirty reads them again (see lib/live.ts).
  const reads = useQueries({
    queries: shown.map((path) => ({
      queryKey: ['tree', taskId, path],
      queryFn: () => loadFolder(path),
      refetchInterval: FALLBACK_POLL_MS,
    })),
  })
  const root = reads[0]
  const folders = new Map<string, TreeEntry[]>()
  shown.forEach((path, index) => {
    const data = reads[index]?.data
    if (data !== undefined) {
      folders.set(path, data)
    }
  })
  const rows = explorerRows(folders, expanded)
  const byKey = new Map(rows.map((row) => [row.key, row]))

  // The same list as the Changes tab.
  const changes = useQuery({
    queryKey: ['changes', taskId],
    queryFn: () => loadChanges(),
    refetchInterval: FALLBACK_POLL_MS,
  })
  const marks = useMemo(() => changeMarks(changes.data ?? []), [changes.data])
  const drafts = useFileDrafts((state) => state.drafts)
  const queryClient = useQueryClient()

  // A file opened from elsewhere (an address, the Changes tab) shows with its folders open.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new open file opens folders.
  useEffect(() => {
    if (openPath === null) {
      return
    }
    const parts = openPath.split('/').slice(0, -1)
    const above = parts.map((_, end) => parts.slice(0, end + 1).join('/'))
    if (above.some((folder) => !expanded.has(folder))) {
      setExpanded(new Set([...expanded, ...above]))
    }
  }, [openPath, taskId])

  const roving = useRovingFocus({
    keys: rows.map((row) => row.key),
    labels: rows.map((row) => row.name),
    preferred: openPath,
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
        if (row.expanded) {
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
        if (row.kind === 'file') {
          onOpen(key)
        } else {
          toggle(key)
        }
        return true
      }
      return false
    },
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-line pr-1 pl-3">
        <span className="min-w-0 flex-1 truncate text-xs text-muted">
          Drag a file onto a terminal to type its path.
        </span>
        <IconButton
          label="Refresh"
          icon={RefreshCw}
          size="xs"
          onClick={() => queryClient.invalidateQueries({ queryKey: ['tree', taskId] })}
        />
      </div>
      {root?.isError ? (
        <p role="alert" className="p-4">
          {root.error.message}
        </p>
      ) : root?.data === undefined ? (
        <SkeletonList rows={8} rowHeight={28} label="Reading the files" />
      ) : (
        <div
          ref={roving.container}
          role="tree"
          aria-label="Files of the worktree"
          {...roving.containerProps}
          className="group/tree min-h-0 flex-1 overflow-y-auto p-1"
        >
          {rows.length === 0 && <p className="m-0 p-3 text-muted">No files.</p>}
          {rows.map((row) => (
            <FileRow
              key={row.key}
              row={row}
              taskId={taskId}
              status={marks.files.get(row.key)}
              changedBelow={marks.folders.has(row.key)}
              unsaved={drafts[draftKey(taskId, row.key)] !== undefined}
              selected={row.key === openPath}
              item={roving.item(row.key)}
              onOpen={onOpen}
              onToggle={toggle}
            />
          ))}
        </div>
      )}
    </div>
  )
}

type FileRowProps = {
  row: ExplorerRow
  taskId: string
  status: ChangeEntry['status'] | undefined
  changedBelow: boolean
  unsaved: boolean
  selected: boolean
  item: ReturnType<ReturnType<typeof useRovingFocus>['item']>
  onOpen: (path: string) => void
  onToggle: (path: string) => void
}

function FileRow({
  row,
  taskId,
  status,
  changedBelow,
  unsaved,
  selected,
  item,
  onOpen,
  onToggle,
}: FileRowProps) {
  const isFile = row.kind === 'file'
  const tone = status === undefined ? 'text-ink-2' : STATUS_TONE[status]
  const label = [row.name, status && CHANGE_LABEL[status], unsaved && 'not saved']
    .filter(Boolean)
    .join(', ')
  return (
    // The tree handles the keys of every row (useRovingFocus).
    // biome-ignore lint/a11y/useKeyWithClickEvents: see above.
    <div
      role="treeitem"
      {...item}
      tabIndex={item.tabIndex}
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(
          pathDragType(taskId),
          JSON.stringify({ path: row.key, kind: row.kind }),
        )
        event.dataTransfer.setData('text/plain', row.key)
        event.dataTransfer.effectAllowed = 'copy'
      }}
      onClick={() => (isFile ? onOpen(row.key) : onToggle(row.key))}
      aria-level={row.level}
      aria-posinset={row.posInSet}
      aria-setsize={row.setSize}
      aria-selected={isFile ? selected : undefined}
      aria-expanded={row.expandable ? row.expanded : undefined}
      aria-label={label}
      style={{ paddingLeft: `calc(0.25rem + ${row.level - 1} * 0.75rem)` }}
      className="cv-row focus-inset relative flex h-(--row-h) cursor-pointer items-center gap-1.5 rounded-md pr-2 select-none hover:bg-hover aria-selected:bg-selected group-focus-within/tree:aria-selected:bg-selection-active"
    >
      <span
        aria-hidden="true"
        className="flex size-4 shrink-0 items-center justify-center text-muted"
      >
        {isFile ? (
          <FileIcon path={row.key} />
        ) : (
          <ChevronRight
            size={ICON.xs}
            className={`motion-safe:transition-transform ${row.expanded ? 'rotate-90' : ''}`}
          />
        )}
      </span>
      <span data-tip={row.key} className={`min-w-0 truncate ${tone}`}>
        {row.name}
      </span>
      <span className="flex-1" />
      {unsaved && <span className="shrink-0 text-2xs text-signal">edited</span>}
      {status !== undefined && (
        <span
          aria-hidden="true"
          className={`flex size-4 shrink-0 items-center justify-center font-mono text-xs font-semibold ${tone}`}
        >
          {status}
        </span>
      )}
      {!isFile && changedBelow && (
        <span
          aria-hidden="true"
          className="flex size-4 shrink-0 items-center justify-center text-modified"
        >
          <span className="size-1.5 rounded-full bg-current" />
        </span>
      )}
    </div>
  )
}
