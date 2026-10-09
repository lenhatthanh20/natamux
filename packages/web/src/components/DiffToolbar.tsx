import type { ChangeEntry, FileDiff } from '@helmsman/core'
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronUp,
  Columns2,
  Copy,
  FoldVertical,
  ImageIcon,
  Rows2,
  SlidersHorizontal,
  UnfoldVertical,
  WrapText,
} from 'lucide-react'
import { fireAppEvent } from '../lib/app-events'
import { changeCountLabel, countLines, splitPath, wholeFileKind } from '../lib/changes'
import { FileIcon } from './FileIcon'
import { copyText } from '../lib/clipboard'
import { shortcutOf } from '../lib/commands'
import { IconButton } from './ui/IconButton'
import { Menu } from './ui/Menu'
import { Segmented } from './ui/Segmented'

// What the body under the toolbar shows: a text diff, a whole new or deleted file, two images,
// a notice (binary, too large), or nothing yet.
export type BodyKind = 'text' | 'whole' | 'image' | 'notice' | 'pending'

type LineCounts = { added: number; removed: number }

function lineLabel(count: number): string {
  return count === 1 ? '1 line' : `${count} lines`
}

// The lines of a new or deleted file, once its text is there.
function wholeLines(entry: ChangeEntry, file: FileDiff | undefined): number | null {
  if (file === undefined || file.path !== entry.path || file.binary || file.tooLarge) {
    return null
  }
  const kind = wholeFileKind(file.status)
  if (kind === 'added') {
    return countLines(file.modified ?? '')
  }
  return kind === 'removed' ? countLines(file.original ?? '') : null
}

// The path of the file, folder muted and name in ink. A renamed file shows where it came from.
export function DiffTitle({ entry, file }: { entry: ChangeEntry; file?: FileDiff }) {
  const { folder, name } = splitPath(entry.path)
  const from = entry.status === 'R' ? entry.originalPath : null
  const lines = wholeLines(entry, file)
  const badge =
    entry.status === 'U' || entry.status === 'A'
      ? 'New file'
      : entry.status === 'D'
        ? 'Deleted'
        : null

  let title = (
    <>
      <span className="text-muted">{folder}</span>
      <span className="text-ink">{name}</span>
    </>
  )
  if (from !== null) {
    const old = splitPath(from)
    title =
      old.folder === folder ? (
        <>
          <span className="text-muted">
            {folder}
            {old.name}
          </span>
          <span className="text-muted"> → </span>
          <span className="text-ink">{name}</span>
        </>
      ) : (
        <>
          <span className="text-muted">{from}</span>
          <span className="text-muted"> → </span>
          <span className="text-ink">{entry.path}</span>
        </>
      )
  }

  return (
    <div className="flex min-w-0 items-center gap-1.5">
      <FileIcon path={entry.path} />
      <span
        data-tip-overflow={from === null ? true : undefined}
        data-tip={from === null ? entry.path : `Renamed from ${from} to ${entry.path}`}
        className="min-w-0 truncate font-mono text-xs"
      >
        {title}
      </span>
      <IconButton
        label="Copy path"
        icon={Copy}
        size="xs"
        onClick={() => copyText(entry.path, 'the path')}
      />
      {badge !== null && (
        <span className="shrink-0 rounded border border-line px-1.5 text-xs whitespace-nowrap text-ink-2">
          {lines === null ? badge : `${badge} · ${lineLabel(lines)}`}
        </span>
      )}
    </div>
  )
}

// Lines added and removed. A new or deleted file says it in its badge instead.
export function DiffCounts({ entry, stats }: { entry: ChangeEntry; stats: LineCounts | null }) {
  if (wholeFileKind(entry.status) !== null || stats === null) {
    return null
  }
  return (
    <span className="font-mono text-xs whitespace-nowrap tabular-nums">
      <span className="text-add">+{stats.added}</span>{' '}
      <span className="text-remove">−{stats.removed}</span>
    </span>
  )
}

// Up and down through the changes of the file, with where the cursor is.
function ChangeNavigator({ index, total }: { index: number; total: number }) {
  return (
    <div className="inline-flex shrink-0 items-center rounded-md border border-line">
      <IconButton
        label="Previous change"
        icon={ArrowUp}
        size="xs"
        shortcut={shortcutOf('diff.previousChange')}
        onClick={() => fireAppEvent('diff-change-step', -1)}
      />
      <span
        aria-live="polite"
        className="min-w-14 text-center font-mono text-xs text-muted tabular-nums"
      >
        {changeCountLabel(index, total)}
      </span>
      <IconButton
        label="Next change"
        icon={ArrowDown}
        size="xs"
        shortcut={shortcutOf('diff.nextChange')}
        onClick={() => fireAppEvent('diff-change-step', 1)}
      />
    </div>
  )
}

export type DiffToolbarProps = {
  entry: ChangeEntry
  file: FileDiff | undefined
  body: BodyKind
  // From the text diff of this file; null until it is worked out.
  stats: LineCounts | null
  changes: { index: number; total: number } | null
  inline: boolean
  // Why side by side cannot be picked now, or undefined when it can.
  sideBySideBlocked?: string
  onSideBySide: (sideBySide: boolean) => void
  // Icons only, for a narrow diff column.
  compact: boolean
  collapseUnchanged: boolean
  onCollapseUnchanged: (collapse: boolean) => void
  wrap: boolean
  onWrap: (wrap: boolean) => void
  // Only for an SVG file shown as text: whether its image shows instead.
  svgPreview: boolean | null
  onSvgPreview: (preview: boolean) => void
  ignoreWhitespace: boolean
  onIgnoreWhitespace: (ignore: boolean) => void
  showMoves: boolean
  onShowMoves: (show: boolean) => void
  canStepFiles: boolean
  onStepFile: (by: 1 | -1) => void
}

export function DiffToolbar({
  entry,
  file,
  body,
  stats,
  changes,
  inline,
  sideBySideBlocked,
  onSideBySide,
  compact,
  collapseUnchanged,
  onCollapseUnchanged,
  wrap,
  onWrap,
  svgPreview,
  onSvgPreview,
  ignoreWhitespace,
  onIgnoreWhitespace,
  showMoves,
  onShowMoves,
  canStepFiles,
  onStepFile,
}: DiffToolbarProps) {
  const text = body === 'text'
  return (
    <div className="flex min-h-9 shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-3 py-1">
      <DiffTitle entry={entry} file={file} />
      <span className="flex-1" />
      <DiffCounts entry={entry} stats={stats} />
      {text && changes !== null && changes.total > 0 && (
        <ChangeNavigator index={changes.index} total={changes.total} />
      )}
      {(text || body === 'image') && (
        <Segmented
          label="Diff layout"
          value={inline ? 'inline' : 'side'}
          onChange={(value) => onSideBySide(value === 'side')}
          iconOnly={compact}
          options={[
            {
              value: 'side',
              label: 'Side by side',
              icon: Columns2,
              disabledReason: sideBySideBlocked,
            },
            { value: 'inline', label: body === 'image' ? 'Stacked' : 'Inline', icon: Rows2 },
          ]}
        />
      )}
      {text && (
        <IconButton
          label="Collapse unchanged lines"
          icon={collapseUnchanged ? UnfoldVertical : FoldVertical}
          aria-pressed={collapseUnchanged}
          onClick={() => onCollapseUnchanged(!collapseUnchanged)}
        />
      )}
      {(text || body === 'whole') && (
        <IconButton
          label="Wrap long lines"
          icon={WrapText}
          aria-pressed={wrap}
          onClick={() => onWrap(!wrap)}
        />
      )}
      {svgPreview !== null && (
        <IconButton
          label="Show image preview"
          icon={ImageIcon}
          aria-pressed={svgPreview}
          onClick={() => onSvgPreview(!svgPreview)}
        />
      )}
      {text && (
        <Menu
          label="Diff options"
          icon={SlidersHorizontal}
          size="sm"
          items={[
            {
              label: 'Ignore leading and trailing whitespace',
              checked: ignoreWhitespace,
              onSelect: () => onIgnoreWhitespace(!ignoreWhitespace),
            },
            {
              label: 'Show moved code',
              checked: showMoves,
              onSelect: () => onShowMoves(!showMoves),
            },
          ]}
        />
      )}
      <IconButton
        label="Previous file"
        icon={ChevronUp}
        shortcut={shortcutOf('diff.previous')}
        disabled={!canStepFiles}
        onClick={() => onStepFile(-1)}
      />
      <IconButton
        label="Next file"
        icon={ChevronDown}
        shortcut={shortcutOf('diff.next')}
        disabled={!canStepFiles}
        onClick={() => onStepFile(1)}
      />
    </div>
  )
}
