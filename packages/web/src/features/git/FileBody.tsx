import type { FileDiff, ImageSide, ReviewComment } from '@natamux/core'
import { lazy } from 'react'
import { Button } from '../../components/ui/Button'
import { LazyView } from '../../components/ui/LazyView'
import { SkeletonList } from '../../components/ui/Skeleton'
import { type CodeSelection, wholeFileKind } from '../../lib/changes'
import { showsImage, wholeText } from './body-kind'
import { ImageDiff } from './ImageDiff'
import type { RevealLine, ZoneActions } from './review-zones'

// Monaco is large. It is loaded the first time a diff is shown, not with the dashboard.
const MonacoDiff = lazy(() => import('./MonacoDiff'))
const MonacoFile = lazy(() => import('../files/MonacoFile'))

export type LineStats = { added: number; removed: number }
export type ChangeInfo = { index: number; total: number }

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
  onAsk: (selection: CodeSelection) => void
  onStats: (stats: LineStats) => void
  onChanges: (info: ChangeInfo) => void
  // The comments to show inside the diff, with the buttons of each.
  reviewComments?: readonly ReviewComment[]
  reviewActions?: ZoneActions
  reveal?: RevealLine | null
}

export function FileBody({
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
  onAsk,
  onStats,
  onChanges,
  reviewComments,
  reviewActions,
  reveal,
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
        <MonacoFile
          path={file.path}
          text={whole}
          kind={kind}
          wrap={wrap}
          onLine={onLine}
          onAsk={onAsk}
        />
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
          onAsk={onAsk}
          onStats={onStats}
          onChanges={onChanges}
          reviewComments={reviewComments}
          reviewActions={reviewActions}
          reveal={reveal}
        />
      </LazyView>
    </>
  )
}
