import type { ChangeEntry, ChangeStatus } from '@natamux/core'
import { ChevronRight } from 'lucide-react'
import { memo } from 'react'
import { FileIcon } from '../../components/FileIcon'
import { ICON } from '../../components/ui/styles'
import { rovingItem } from '../../hooks/useRovingFocus'
import { CHANGE_LABEL, countLabel, splitPath } from '../../lib/changes'
import { entryCounts, type TreeRow } from './change-tree'
import type { FileViewMode } from './useFileViewMode'

export const STATUS_TONE: Record<ChangeStatus, string> = {
  M: 'text-modified',
  A: 'text-add',
  U: 'text-add',
  D: 'text-remove',
  C: 'text-remove',
  R: 'text-renamed',
}

export type ChangedRowProps = {
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
export const ChangedRow = memo(function ChangedRow({
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
