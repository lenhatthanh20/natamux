import { type ChangeEntry, type FileDiff, type ImageSide, imageTypeOf } from '@natamux/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { FileDiff as FileDiffIcon, RefreshCw, Sparkles } from 'lucide-react'
import { type KeyboardEvent, lazy, useEffect, useMemo, useRef, useState } from 'react'
import { useElementWidth } from '../hooks/useElementWidth'
import { useFileViewMode } from '../hooks/useFileViewMode'
import { useStoredFlag } from '../hooks/useStoredFlag'
import { useAppEvent } from '../lib/app-events'
import { fileOrder } from '../lib/change-tree'
import { stepFile, wholeFileKind, wrapsByDefault } from '../lib/changes'
import { AgentComment } from './AgentComment'
import { ChangedFiles } from './ChangedFiles'
import { type BodyKind, DiffToolbar } from './DiffToolbar'
import { ImageDiff } from './ImageDiff'
import { Button } from './ui/Button'
import { EmptyState } from './ui/EmptyState'
import { IconButton } from './ui/IconButton'
import { LazyView } from './ui/LazyView'
import { ResizeHandle, useStoredWidth } from './ui/ResizeHandle'
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
// Under this width the layout switch shows icons only.
const FULL_LABELS_FROM = 960
const NO_CHANGES: ChangeEntry[] = []

type ChangesPanelProps = {
  taskId: string
  // `fresh`: the daemon reads git now, not the answer it kept for timed reads.
  loadChanges: (fresh?: boolean) => Promise<ChangeEntry[]>
  loadFile: (path: string) => Promise<FileDiff>
  loadImage: (path: string, side: ImageSide) => Promise<Blob>
  sendToAgent: (text: string) => Promise<unknown>
  // Commits and says so. Resolves true when it worked, so the box can empty itself.
  onCommit: (message: string, fileCount: number) => Promise<boolean>
  committing: boolean
  // Another git action is running: commit waits.
  gitBusy: boolean
  // Asks the agent for a commit message. Null when it failed; the caller says why.
  onDraft: (() => Promise<string | null>) | undefined
  // The name of the agent that drafts, as the user knows it.
  draftAgent?: string
  // Commit… was picked: draft a message when the box is empty, then call onAutoDraftTaken.
  autoDraft?: boolean
  onAutoDraftTaken?: () => void
  // The file of the address bar, and where to report the one that shows.
  file?: string | null
  onFileChange?: (path: string | null) => void
}

type LineStats = { added: number; removed: number }
type ChangeInfo = { index: number; total: number }

// An image that cannot be shown as text, or an SVG that the reader wants to see drawn.
function showsImage(file: FileDiff, svgPreview: boolean): boolean {
  return imageTypeOf(file.path) !== null && (file.binary || file.tooLarge || svgPreview)
}

// Must match what FileBody shows, so the toolbar has no buttons that do nothing.
function bodyKindOf(file: FileDiff | undefined, svgPreview: boolean, sameText: boolean): BodyKind {
  if (file === undefined) {
    return 'pending'
  }
  if (showsImage(file, svgPreview)) {
    return 'image'
  }
  if (file.binary || file.tooLarge) {
    return 'notice'
  }
  const kind = wholeFileKind(file.status)
  if (kind !== null) {
    return wholeText(file, kind) === '' ? 'notice' : 'whole'
  }
  return sameText ? 'notice' : 'text'
}

function wholeText(file: FileDiff, kind: 'added' | 'removed'): string {
  return (kind === 'added' ? file.modified : file.original) ?? ''
}

function Notice({ children }: { children: string }) {
  return (
    <p role="status" className="p-4 text-muted">
      {children}
    </p>
  )
}

type FileBodyProps = {
  taskId: string
  file: FileDiff
  sideBySide: boolean
  wrap: boolean
  collapseUnchanged: boolean
  ignoreWhitespace: boolean
  showMoves: boolean
  svgPreview: boolean
  // Both sides have the same text.
  sameText: boolean
  // The diff found no change, but the texts differ: only whitespace or line endings changed.
  whitespaceOnly: boolean
  onShowWhitespace: () => void
  loadImage: (path: string, side: ImageSide) => Promise<Blob>
  onLine: (line: number) => void
  onStats: (stats: LineStats) => void
  onChanges: (info: ChangeInfo) => void
}

function FileBody({
  taskId,
  file,
  sideBySide,
  wrap,
  collapseUnchanged,
  ignoreWhitespace,
  showMoves,
  svgPreview,
  sameText,
  whitespaceOnly,
  onShowWhitespace,
  loadImage,
  onLine,
  onStats,
  onChanges,
}: FileBodyProps) {
  // Keyed by path: the size of the image before must not show while the new one loads.
  if (showsImage(file, svgPreview)) {
    return (
      <ImageDiff
        key={file.path}
        taskId={taskId}
        path={file.path}
        status={file.status}
        layout={sideBySide ? 'side' : 'stacked'}
        loadImage={loadImage}
      />
    )
  }
  if (file.binary) {
    return <Notice>This file is binary. It changed, but it is not shown.</Notice>
  }
  if (file.tooLarge) {
    return <Notice>This file is larger than 1 MB, so it is not shown.</Notice>
  }
  const viewer = <SkeletonList rows={6} rowHeight={28} label="Loading the viewer" />
  const kind = wholeFileKind(file.status)
  if (kind !== null) {
    const whole = wholeText(file, kind)
    if (whole === '') {
      return <Notice>Empty file.</Notice>
    }
    return (
      <LazyView label="viewer" fallback={viewer}>
        <MonacoFile path={file.path} text={whole} kind={kind} wrap={wrap} onLine={onLine} />
      </LazyView>
    )
  }
  if (sameText) {
    return (
      <Notice>
        {file.status === 'R'
          ? 'Renamed without changes.'
          : 'The text did not change. Only the file mode may differ.'}
      </Notice>
    )
  }
  return (
    <>
      {whitespaceOnly && (
        <div
          role="status"
          className="flex shrink-0 items-center gap-3 border-b border-line px-3 py-1.5 text-muted"
        >
          <span className="min-w-0 flex-1">
            {ignoreWhitespace
              ? 'Only whitespace or line endings changed.'
              : 'Only line endings changed.'}
          </span>
          {ignoreWhitespace && (
            <Button size="sm" onClick={onShowWhitespace}>
              Show whitespace
            </Button>
          )}
        </div>
      )}
      <LazyView label="viewer" fallback={viewer}>
        <MonacoDiff
          path={file.path}
          original={file.original ?? ''}
          modified={file.modified ?? ''}
          sideBySide={sideBySide}
          wrap={wrap}
          collapseUnchanged={collapseUnchanged}
          ignoreWhitespace={ignoreWhitespace}
          showMoves={showMoves}
          onLine={onLine}
          onStats={onStats}
          onChanges={onChanges}
        />
      </LazyView>
    </>
  )
}

// The Changes tab: the changed files, the diff of one of them, and the boxes to comment and commit.
export function ChangesPanel({
  taskId,
  loadChanges,
  loadFile,
  loadImage,
  sendToAgent,
  onCommit,
  committing,
  gitBusy,
  onDraft,
  draftAgent = 'the task agent',
  autoDraft = false,
  onAutoDraftTaken,
  file: requestedFile = null,
  onFileChange,
}: ChangesPanelProps) {
  const [selectedPath, setSelectedPath] = useState<string | null>(requestedFile)
  const [sideBySide, setSideBySide] = useStoredFlag('natamux.diff-side-by-side', true)
  const [collapseUnchanged, setCollapseUnchanged] = useStoredFlag(
    'natamux.diff-collapse-unchanged',
    true,
  )
  const [ignoreWhitespace, setIgnoreWhitespace] = useStoredFlag(
    'natamux.diff-ignore-whitespace',
    true,
  )
  const [showMoves, setShowMoves] = useStoredFlag('natamux.diff-show-moves', false)
  const [wrapProse, setWrapProse] = useStoredFlag('natamux.diff-wrap-prose', true)
  const [wrapCode, setWrapCode] = useStoredFlag('natamux.diff-wrap-code', false)
  // The SVG file whose image shows instead of its text.
  const [previewPath, setPreviewPath] = useState<string | null>(null)
  const [message, setMessage] = useState('')
  const [drafting, setDrafting] = useState(false)
  const messageBox = useRef<HTMLTextAreaElement>(null)
  const [line, setLine] = useState<number | null>(null)
  const [stats, setStats] = useState<({ path: string } & LineStats) | null>(null)
  const [changeInfo, setChangeInfo] = useState<({ path: string } & ChangeInfo) | null>(null)
  const [viewMode, setViewMode] = useFileViewMode()
  const [rootRef, rootWidth] = useElementWidth<HTMLDivElement>()
  const files = useStoredWidth('natamux.files-width', 260, 200, 420)
  const queryClient = useQueryClient()

  // Set while a refresh the user asked for runs.
  const fresh = useRef(false)
  const changes = useQuery({
    queryKey: ['changes', taskId],
    queryFn: () => loadChanges(fresh.current),
    refetchInterval: REFRESH_MS,
  })
  const list = changes.data ?? NO_CHANGES
  // The order the file panel shows, so [ and ] move the way the eye reads it.
  const order = useMemo(() => fileOrder(list, viewMode), [list, viewMode])
  const current =
    list.find((entry) => entry.path === selectedPath) ??
    list.find((entry) => entry.path === order[0]) ??
    null

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
  const shown = current !== null && file.data?.path === current.path ? file.data : undefined
  const sameText = useMemo(() => shown !== undefined && shown.original === shown.modified, [shown])

  // Unmeasured counts as wide: the first frame has the file panel, and tests have no layout.
  const wide = rootWidth === null || rootWidth >= WIDE_FROM
  const diffWidth =
    rootWidth === null ? Number.POSITIVE_INFINITY : rootWidth - (wide ? files.width : 0)
  const canSideBySide = diffWidth >= SIDE_BY_SIDE_FROM
  // The stored choice stays when the window is narrow, and comes back when it is wide again.
  const inline = !sideBySide || !canSideBySide
  const prose = current === null ? false : wrapsByDefault(current.path)
  const wrap = prose ? wrapProse : wrapCode
  const setWrap = prose ? setWrapProse : setWrapCode
  const isSvg = current !== null && imageTypeOf(current.path) === 'image/svg+xml'
  const svgPreview = isSvg && previewPath === current?.path
  const body = bodyKindOf(shown, svgPreview, sameText)
  const fileStats = current !== null && stats?.path === current.path ? stats : null
  const fileChanges = current !== null && changeInfo?.path === current.path ? changeInfo : null
  // The preview switch is for an SVG that is shown as text. A big SVG is an image anyway.
  const svgSwitch =
    isSvg && shown !== undefined && !shown.binary && !shown.tooLarge ? svgPreview : null

  const choose = (path: string | null) => {
    if (path !== null) {
      setSelectedPath(path)
      setLine(null)
    }
  }
  const step = (by: 1 | -1) => choose(stepFile(order, current?.path ?? null, by))
  // Images are under 'file-diff' too, so they are read again as well.
  const refresh = () => {
    fresh.current = true
    void changes.refetch().finally(() => {
      fresh.current = false
    })
    void queryClient.invalidateQueries({ queryKey: ['file-diff', taskId] })
  }

  // [ and ] move between files, Shift+R reads again, Mod+Shift+A goes to the comment box. The keys
  // are commands, see lib/commands.ts.
  useAppEvent<number>('diff-step', (by) => step(by < 0 ? -1 : 1))
  useAppEvent('refresh-changes', refresh)
  useAppEvent('diff-comment', () => document.getElementById('agent-comment')?.focus())

  // The address bar names the file, and follows the one that shows.
  // Only a new address moves the selection, so selectedPath is not a trigger.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above.
  useEffect(() => {
    if (requestedFile !== null && requestedFile !== selectedPath) {
      setSelectedPath(requestedFile)
    }
  }, [requestedFile])
  const shownPath = current?.path ?? null
  useEffect(() => onFileChange?.(shownPath), [shownPath, onFileChange])

  const draft = async () => {
    if (onDraft === undefined || drafting) {
      return
    }
    const before = message
    setDrafting(true)
    try {
      const drafted = await onDraft()
      if (drafted !== null) {
        // Text typed while the agent writes wins over the draft.
        setMessage((now) => (now === before ? drafted : now))
        requestAnimationFrame(() => {
          const box = messageBox.current
          box?.focus()
          box?.setSelectionRange(box.value.length, box.value.length)
        })
      }
    } finally {
      setDrafting(false)
    }
  }
  useAppEvent('commit-draft', () => void draft())
  // Commit… waits for the list, so it knows whether there is anything to draft for.
  const loaded = changes.data !== undefined
  // biome-ignore lint/correctness/useExhaustiveDependencies: a request or the list starts it.
  useEffect(() => {
    if (autoDraft && loaded) {
      onAutoDraftTaken?.()
      if (list.length > 0 && message.trim() === '') {
        void draft()
      }
    }
  }, [autoDraft, loaded])

  const submitCommit = async (event: { preventDefault: () => void }) => {
    event.preventDefault()
    if (message.trim() === '' || committing || gitBusy || drafting) {
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
        ref={messageBox}
        id="commit-message"
        rows={3}
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={commitOnShortcut}
        placeholder="Commit message"
        className={`${textarea} resize-none`}
      />
      <div className="flex gap-2">
        {onDraft !== undefined && (
          <Button
            variant="ghost"
            size="md"
            icon={Sparkles}
            busy={drafting}
            data-tip={`Draft a message with ${draftAgent}`}
            onClick={() => void draft()}
          >
            Generate
          </Button>
        )}
        <Button
          type="submit"
          variant="primary"
          size="md"
          className="flex-1"
          busy={committing}
          disabled={gitBusy && !committing}
          disabledReason={
            drafting
              ? 'Drafting the message…'
              : message.trim() === ''
                ? 'Write a commit message first'
                : undefined
          }
          shortcut="Mod+Enter"
        >
          Commit all
        </Button>
      </div>
    </form>
  )

  const toolbar = current !== null && (
    <DiffToolbar
      entry={current}
      file={shown}
      body={body}
      stats={fileStats}
      changes={fileChanges}
      inline={inline}
      sideBySideBlocked={canSideBySide ? undefined : 'Needs a wider window'}
      onSideBySide={setSideBySide}
      compact={diffWidth < FULL_LABELS_FROM}
      collapseUnchanged={collapseUnchanged}
      onCollapseUnchanged={setCollapseUnchanged}
      wrap={wrap}
      onWrap={setWrap}
      svgPreview={svgSwitch}
      onSvgPreview={(preview) => setPreviewPath(preview ? current.path : null)}
      ignoreWhitespace={ignoreWhitespace}
      onIgnoreWhitespace={setIgnoreWhitespace}
      showMoves={showMoves}
      onShowMoves={setShowMoves}
      canStepFiles={order.length >= 2}
      onStepFile={step}
    />
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
        {current !== null && shown !== undefined && (
          <FileBody
            taskId={taskId}
            file={shown}
            sideBySide={!inline}
            wrap={wrap}
            collapseUnchanged={collapseUnchanged}
            ignoreWhitespace={ignoreWhitespace}
            showMoves={showMoves}
            svgPreview={svgPreview}
            sameText={sameText}
            whitespaceOnly={fileChanges?.total === 0}
            onShowWhitespace={() => setIgnoreWhitespace(false)}
            loadImage={loadImage}
            onLine={setLine}
            onStats={(next) => setStats({ path: current.path, ...next })}
            onChanges={(next) => setChangeInfo({ path: current.path, ...next })}
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

  const byPath = new Map(list.map((entry) => [entry.path, entry]))
  return (
    <div ref={rootRef} className={`flex min-h-0 flex-1 ${wide ? '' : 'flex-col overflow-y-auto'}`}>
      {wide ? (
        <aside
          aria-label="Files"
          style={{ width: files.width }}
          className="flex shrink-0 flex-col bg-surface"
        >
          <ChangedFiles
            taskId={taskId}
            list={list}
            loaded={changes.data !== undefined}
            mode={viewMode}
            onModeChange={setViewMode}
            selectedPath={current?.path ?? null}
            onSelect={choose}
            onRefresh={refresh}
            status={
              <>
                {changes.isError && (
                  <p role="alert" className="p-3">
                    {changes.error.message}
                  </p>
                )}
                {changes.isPending && (
                  <SkeletonList rows={6} rowHeight={28} label="Reading changes" />
                )}
              </>
            }
          />
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
            {order.map((path) => (
              <option key={path} value={path}>
                {byPath.get(path)?.status} {path}
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
