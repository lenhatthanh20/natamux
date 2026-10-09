import type { ChangeEntry, FileDiff } from '@helmsman/core'
import { useQuery } from '@tanstack/react-query'
import {
  ChevronDown,
  ChevronUp,
  Columns2,
  FileDiff as FileDiffIcon,
  RefreshCw,
  Rows2,
  WrapText,
} from 'lucide-react'
import { type KeyboardEvent, lazy, Suspense, useEffect, useRef, useState } from 'react'
import { useElementWidth } from '../hooks/useElementWidth'
import {
  CHANGE_LABEL,
  countLabel,
  countLines,
  splitPath,
  stepFile,
  wholeFileKind,
  wrapsByDefault,
} from '../lib/changes'
import { AgentComment } from './AgentComment'
import { Button } from './ui/Button'
import { EmptyState } from './ui/EmptyState'
import { IconButton } from './ui/IconButton'
import { ResizeHandle, useStoredWidth } from './ui/ResizeHandle'
import { Segmented } from './ui/Segmented'
import { Select } from './ui/Select'
import { SkeletonList } from './ui/Skeleton'
import { textarea } from './ui/styles'

// Monaco is large. It is loaded the first time a diff is shown, not with the dashboard.
const MonacoDiff = lazy(() => import('./MonacoDiff'))
const MonacoFile = lazy(() => import('./MonacoFile'))

const REFRESH_MS = 10_000
// Under this width the file list becomes a picker above the diff.
const WIDE_FROM = 720
// Side by side needs this much room for the diff itself.
const SIDE_BY_SIDE_FROM = 720

const LETTER_COLOR: Record<ChangeEntry['status'], string> = {
  M: 'text-modified',
  A: 'text-add',
  U: 'text-add',
  D: 'text-remove',
  C: 'text-remove',
  R: 'text-renamed',
}

type ChangesPanelProps = {
  taskId: string
  loadChanges: () => Promise<ChangeEntry[]>
  loadFile: (path: string) => Promise<FileDiff>
  sendToAgent: (text: string) => Promise<unknown>
  // Commits and says so. Resolves true when it worked, so the box can empty itself.
  onCommit: (message: string, fileCount: number) => Promise<boolean>
  committing: boolean
  // Another git action is running: commit waits.
  gitBusy: boolean
}

function FileBody({
  file,
  sideBySide,
  wrap,
  onLine,
  onStats,
}: {
  file: FileDiff
  sideBySide: boolean
  wrap: boolean
  onLine: (line: number) => void
  onStats: (stats: { added: number; removed: number }) => void
}) {
  if (file.binary) {
    return <p className="p-4 text-muted">This file is binary. It changed, but it is not shown.</p>
  }
  if (file.tooLarge) {
    return <p className="p-4 text-muted">This file is larger than 1 MB, so it is not shown.</p>
  }
  const kind = wholeFileKind(file.status)
  const whole = kind === 'added' ? file.modified : kind === 'removed' ? file.original : null
  return (
    <Suspense fallback={<SkeletonList rows={6} rowHeight={28} label="Loading the viewer" />}>
      {kind !== null && whole !== null ? (
        <MonacoFile path={file.path} text={whole} kind={kind} wrap={wrap} onLine={onLine} />
      ) : (
        <MonacoDiff
          path={file.path}
          original={file.original ?? ''}
          modified={file.modified ?? ''}
          sideBySide={sideBySide}
          wrap={wrap}
          onLine={onLine}
          onStats={onStats}
        />
      )}
    </Suspense>
  )
}

function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName) ||
      target.closest('.xterm, .monaco-editor') !== null)
  )
}

// The Changes tab: the changed files, the diff of one of them, and the boxes to comment and commit.
export function ChangesPanel({
  taskId,
  loadChanges,
  loadFile,
  sendToAgent,
  onCommit,
  committing,
  gitBusy,
}: ChangesPanelProps) {
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [sideBySide, setSideBySide] = useState(true)
  const [wrapChoice, setWrapChoice] = useState<boolean | null>(null)
  const [message, setMessage] = useState('')
  const [line, setLine] = useState<number | null>(null)
  const [stats, setStats] = useState<{ path: string; added: number; removed: number } | null>(null)
  const [rootRef, rootWidth] = useElementWidth<HTMLDivElement>()
  const files = useStoredWidth('helmsman.files-width', 260, 200, 420)

  const changes = useQuery({
    queryKey: ['changes', taskId],
    queryFn: loadChanges,
    refetchInterval: REFRESH_MS,
  })
  const list = changes.data ?? []
  const current = list.find((entry) => entry.path === selectedPath) ?? list[0] ?? null

  const file = useQuery({
    queryKey: ['file-diff', taskId, current?.path, current?.status],
    queryFn: async () => {
      if (!current) {
        throw new Error('No file is chosen.')
      }
      return loadFile(current.path)
    },
    enabled: current !== null,
    refetchInterval: REFRESH_MS,
  })

  // Unmeasured counts as wide: the first frame has the file panel, and tests have no layout.
  const wide = rootWidth === null || rootWidth >= WIDE_FROM
  const diffWidth =
    rootWidth === null ? Number.POSITIVE_INFINITY : rootWidth - (wide ? files.width : 0)
  const canSideBySide = diffWidth >= SIDE_BY_SIDE_FROM
  const inline = !sideBySide || !canSideBySide
  const wrap = wrapChoice ?? (current === null ? false : wrapsByDefault(current.path))

  const choose = (path: string | null) => {
    if (path !== null) {
      setSelectedPath(path)
      setLine(null)
    }
  }
  const step = (by: 1 | -1) =>
    choose(
      stepFile(
        list.map((entry) => entry.path),
        current?.path ?? null,
        by,
      ),
    )
  const refresh = () => {
    changes.refetch()
    file.refetch()
  }

  // [ and ] move between files, Shift+R reads again. Not while the user types or reads a diff.
  const keys = useRef({ step, refresh })
  keys.current = { step, refresh }
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || isTypingTarget(event.target)) {
        return
      }
      if (event.key === '[') {
        keys.current.step(-1)
      } else if (event.key === ']') {
        keys.current.step(1)
      } else if (event.key === 'R') {
        keys.current.refresh()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const submitCommit = async (event: { preventDefault: () => void }) => {
    event.preventDefault()
    if (message.trim() === '' || committing || gitBusy) {
      return
    }
    if (await onCommit(message.trim(), list.length)) {
      setMessage('')
    }
  }
  const commitOnShortcut = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      void submitCommit(event)
    }
  }

  const commitBox = list.length > 0 && (
    <form
      onSubmit={(event) => void submitCommit(event)}
      className="flex shrink-0 flex-col gap-2 border-t border-line bg-surface p-2"
    >
      <label htmlFor="commit-message" className="sr-only">
        Commit message
      </label>
      <textarea
        id="commit-message"
        rows={3}
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={commitOnShortcut}
        placeholder="Commit message"
        className={`${textarea} resize-none`}
      />
      <Button
        type="submit"
        variant="primary"
        size="md"
        busy={committing}
        disabled={gitBusy && !committing}
        disabledReason={message.trim() === '' ? 'Write a commit message first' : undefined}
        shortcut="Mod+Enter"
      >
        Commit all
      </Button>
    </form>
  )

  const toolbar = current !== null && (
    <div className="flex min-h-9 shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-3 py-1">
      <DiffTitle entry={current} />
      <span className="flex-1" />
      <DiffCounts
        path={current.path}
        file={file.data}
        stats={stats?.path === current.path ? stats : null}
      />
      <Segmented
        label="Diff layout"
        value={inline ? 'inline' : 'side'}
        onChange={(value) => setSideBySide(value === 'side')}
        options={[
          {
            value: 'side',
            label: 'Side by side',
            icon: Columns2,
            disabledReason: canSideBySide ? undefined : 'Needs a wider window',
          },
          { value: 'inline', label: 'Inline', icon: Rows2 },
        ]}
      />
      <IconButton
        label="Wrap long lines"
        icon={WrapText}
        aria-pressed={wrap}
        onClick={() => setWrapChoice(!wrap)}
      />
      <IconButton
        label="Previous file"
        icon={ChevronUp}
        shortcut="["
        disabled={list.length < 2}
        onClick={() => step(-1)}
      />
      <IconButton
        label="Next file"
        icon={ChevronDown}
        shortcut="]"
        disabled={list.length < 2}
        onClick={() => step(1)}
      />
    </div>
  )

  const diff = (
    <div className="flex min-w-0 flex-1 flex-col">
      {toolbar}
      <div className={`flex min-w-0 flex-1 flex-col ${wide ? 'min-h-0' : 'min-h-80'}`}>
        {current !== null && file.isPending && (
          <SkeletonList rows={6} rowHeight={28} label="Reading the file" />
        )}
        {file.isError && (
          <p role="alert" className="p-4">
            {file.error.message}
          </p>
        )}
        {current !== null && file.data && (
          <FileBody
            file={file.data}
            sideBySide={!inline}
            wrap={wrap}
            onLine={setLine}
            onStats={(next) => setStats({ path: current.path, ...next })}
          />
        )}
      </div>
      {current !== null && <AgentComment path={current.path} line={line} send={sendToAgent} />}
    </div>
  )

  if (changes.data && list.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center">
        <EmptyState icon={FileDiffIcon} title="No changes">
          The workspace matches its last commit.
        </EmptyState>
      </div>
    )
  }

  return (
    <div ref={rootRef} className={`flex min-h-0 flex-1 ${wide ? '' : 'flex-col overflow-y-auto'}`}>
      {wide ? (
        <aside
          aria-label="Files"
          style={{ width: files.width }}
          className="flex shrink-0 flex-col bg-surface"
        >
          <div className="flex h-8 shrink-0 items-center gap-2 border-b border-line pr-1 pl-3">
            <span className="flex-1 text-xs font-medium whitespace-nowrap text-muted">
              {changes.data ? countLabel(list.length) : 'Reading changes.'}
            </span>
            <IconButton
              label="Refresh"
              icon={RefreshCw}
              size="xs"
              shortcut="Shift+R"
              onClick={refresh}
            />
          </div>
          {changes.isError && (
            <p role="alert" className="p-3">
              {changes.error.message}
            </p>
          )}
          {changes.isPending && <SkeletonList rows={6} rowHeight={28} label="Reading changes" />}
          <ul aria-label="Changed files" className="min-h-0 flex-1 overflow-y-auto p-1">
            {list.map((entry) => (
              <FileRow
                key={entry.path}
                entry={entry}
                selected={entry.path === current?.path}
                onSelect={() => choose(entry.path)}
              />
            ))}
          </ul>
          {commitBox}
        </aside>
      ) : (
        <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2">
          <Select
            aria-label="Changed file"
            value={current?.path ?? ''}
            onChange={(event) => choose(event.target.value)}
            className="flex-1"
          >
            {list.map((entry) => (
              <option key={entry.path} value={entry.path}>
                {entry.status} {entry.path}
              </option>
            ))}
          </Select>
          <IconButton label="Refresh" icon={RefreshCw} onClick={refresh} />
        </div>
      )}
      {wide && (
        <ResizeHandle
          label="Resize the file list"
          width={files.width}
          min={files.min}
          max={files.max}
          edge="right"
          defaultWidth={260}
          onResize={files.setWidth}
        />
      )}
      {diff}
      {!wide && commitBox}
    </div>
  )
}

function FileRow({
  entry,
  selected,
  onSelect,
}: {
  entry: ChangeEntry
  selected: boolean
  onSelect: () => void
}) {
  const { folder, name } = splitPath(entry.path)
  return (
    <li>
      <button
        type="button"
        onClick={onSelect}
        aria-current={selected}
        className="focus-inset flex h-(--row-h) w-full items-center gap-2 rounded-md px-2 text-left hover:bg-hover aria-[current=true]:bg-selected"
      >
        <span
          data-tip={CHANGE_LABEL[entry.status]}
          className={`flex size-4 shrink-0 items-center justify-center font-mono text-xs font-semibold ${LETTER_COLOR[entry.status]}`}
        >
          {entry.status}
        </span>
        <span className="shrink-0 font-medium">{name}</span>
        {folder !== '' && (
          // The folder loses its start when it is long: the end names the place.
          <span
            data-tip={entry.path}
            className="min-w-0 flex-1 truncate text-xs text-muted"
            style={{ direction: 'rtl', textAlign: 'left' }}
          >
            {`${folder}‎`}
          </span>
        )}
      </button>
    </li>
  )
}

function DiffTitle({ entry }: { entry: ChangeEntry }) {
  const { folder, name } = splitPath(entry.path)
  const badge =
    entry.status === 'U' || entry.status === 'A'
      ? 'New file'
      : entry.status === 'D'
        ? 'Deleted'
        : entry.status === 'R' && entry.originalPath !== null
          ? `Renamed from ${entry.originalPath}`
          : null
  return (
    <div className="flex min-w-0 items-center gap-2">
      <span data-tip-overflow data-tip={entry.path} className="min-w-0 truncate font-mono text-xs">
        <span className="text-muted">{folder}</span>
        <span className="text-ink">{name}</span>
      </span>
      {badge !== null && (
        <span className="max-w-60 shrink-0 truncate rounded border border-line px-1.5 text-xs text-ink-2">
          {badge}
        </span>
      )}
    </div>
  )
}

function DiffCounts({
  path,
  file,
  stats,
}: {
  path: string
  file: FileDiff | undefined
  stats: { added: number; removed: number } | null
}) {
  let counts = stats
  if (file !== undefined && file.path === path) {
    const kind = wholeFileKind(file.status)
    if (kind === 'added') {
      counts = { added: countLines(file.modified ?? ''), removed: 0 }
    } else if (kind === 'removed') {
      counts = { added: 0, removed: countLines(file.original ?? '') }
    }
  }
  if (counts === null) {
    return null
  }
  return (
    <span className="font-mono text-xs whitespace-nowrap">
      <span className="text-add">+{counts.added}</span>{' '}
      <span className="text-remove">−{counts.removed}</span>
    </span>
  )
}
