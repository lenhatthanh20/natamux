import type { ReviewComment } from '@natamux/core'
import type { editor as MonacoEditor } from 'monaco-editor'
import { useEffect, useRef } from 'react'
import { useAppEvent } from '../../lib/app-events'
import {
  type CodeSelection,
  changeAnchor,
  changeAt,
  countLineChanges,
  type LineChange,
  stepChange,
} from '../../lib/changes'
import { TERMINAL_FONT } from '../../lib/fonts'
import { type ResolvedTheme, uiZoom, useResolvedTheme } from '../../lib/theme'
import { useUiSize } from '../../lib/ui-size'
import { languageForPath } from '../files/language'
import { attachAskAgent } from '../files/monaco-ask'
import { DARK_THEME, LIGHT_THEME, monaco } from '../files/monaco-setup'
import { type RevealLine, showReviewZones, type ZoneActions } from './review-zones'

type MonacoDiffProps = {
  path: string
  original: string
  modified: string
  sideBySide: boolean
  wrap: boolean
  // Folds the lines that did not change, with a few lines of context around each change.
  collapseUnchanged: boolean
  // Changes only at the start or end of a line do not count.
  ignoreWhitespace: boolean
  // Code that moved shows as a move, not as a removal and an addition.
  showMoves: boolean
  // Called each time the diff is worked out, with the lines added and removed.
  onStats?: (stats: { added: number; removed: number }) => void
  // The line of the new file that the cursor is on, for a comment about that line.
  onLine?: (line: number) => void
  // Which change the cursor is in (-1 for none), and how many there are.
  onChanges?: (info: { index: number; total: number }) => void
  // Lines selected on either side, to quote to the agent.
  onAsk?: (selection: CodeSelection) => void
  // The comments to show under their lines (this file's, resolved ones already left out).
  reviewComments?: readonly ReviewComment[]
  reviewActions?: ZoneActions
  // Moves the cursor to a line of a side, when a comment in the list was picked. `n` makes the
  // same line count again.
  reveal?: RevealLine | null
}

type Texts = { original: string; modified: string }
type Models = { original: MonacoEditor.ITextModel; modified: MonacoEditor.ITextModel }

// Each value is given again on every update, so the rest of the setting keeps these.
const UNCHANGED_REGIONS = { contextLineCount: 3, minimumLineCount: 3, revealLineCount: 20 }

// The app's theme decides, not the system's: a dark app on a light system still gets a dark diff.
function monacoTheme(theme: ResolvedTheme): string {
  return theme === 'dark' ? DARK_THEME : LIGHT_THEME
}

// The diff of VS Code, read only. This default export is what ChangesView loads on demand.
export default function MonacoDiff({
  path,
  original,
  modified,
  sideBySide,
  wrap,
  collapseUnchanged,
  ignoreWhitespace,
  showMoves,
  onStats,
  onLine,
  onChanges,
  onAsk,
  reviewComments,
  reviewActions,
  reveal,
}: MonacoDiffProps) {
  const container = useRef<HTMLElement>(null)
  const editorRef = useRef<MonacoEditor.IStandaloneDiffEditor | null>(null)
  const modelsRef = useRef<Models | null>(null)
  const latestTexts = useRef<Texts>({ original, modified })
  latestTexts.current = { original, modified }
  // The texts the models hold, so a refresh with the same text costs nothing.
  const shownTexts = useRef<Texts | null>(null)
  const onLineRef = useRef(onLine)
  onLineRef.current = onLine
  const onStatsRef = useRef(onStats)
  onStatsRef.current = onStats
  const onChangesRef = useRef(onChanges)
  onChangesRef.current = onChanges
  const onAskRef = useRef(onAsk)
  onAskRef.current = onAsk
  const actionsRef = useRef(reviewActions)
  actionsRef.current = reviewActions
  const wrapRef = useRef(wrap)
  wrapRef.current = wrap
  const sideBySideRef = useRef(sideBySide)
  sideBySideRef.current = sideBySide
  const collapseRef = useRef(collapseUnchanged)
  collapseRef.current = collapseUnchanged
  const ignoreRef = useRef(ignoreWhitespace)
  ignoreRef.current = ignoreWhitespace
  const movesRef = useRef(showMoves)
  movesRef.current = showMoves
  const theme = useResolvedTheme()
  const uiSize = useUiSize()
  const themeRef = useRef(theme)
  themeRef.current = theme

  const changesRef = useRef<readonly LineChange[]>([])
  // The cursor line of the new side. Null until the user moves the cursor or steps to a change, so
  // the first step goes to the first change and a comment is about the whole file.
  const lineRef = useRef<number | null>(null)
  // The file whose first change was scrolled to. Set only once it happens: in dev, StrictMode
  // builds the editor twice, and the first one is gone before its diff is done.
  const revealedPath = useRef<string | null>(null)
  // What onChanges got last. A cursor move inside the same change must not render the panel again.
  const lastReported = useRef<{ index: number; total: number } | null>(null)
  // Monaco moves the cursor when it gets a model or new text. Those moves are not the user's.
  const quiet = useRef(false)
  // One key press can reach both the app's shortcut and the editor's own key. It steps once.
  const stepping = useRef(false)

  const report = () => {
    const changes = changesRef.current
    const line = lineRef.current
    const index = line === null ? -1 : changeAt(changes, line)
    const last = lastReported.current
    if (last !== null && last.index === index && last.total === changes.length) {
      return
    }
    const info = { index, total: changes.length }
    lastReported.current = info
    onChangesRef.current?.(info)
  }

  const step = (by: 1 | -1) => {
    const editor = editorRef.current
    if (editor === null || stepping.current) {
      return
    }
    stepping.current = true
    window.setTimeout(() => {
      stepping.current = false
    }, 0)
    const changes = changesRef.current
    const change = changes[stepChange(changes, lineRef.current, by)]
    if (change === undefined) {
      return
    }
    const { start } = changeAnchor(change)
    const target = editor.getModifiedEditor()
    // The focus stays where it is: a button in the toolbar keeps it, and so does the editor.
    target.setPosition({ lineNumber: start, column: 1 })
    target.revealLineInCenterIfOutsideViewport(start)
    // Monaco tells nothing when the cursor is on that line already, so this does not wait for it.
    lineRef.current = start
    onLineRef.current?.(start)
    report()
  }
  const stepRef = useRef(step)
  stepRef.current = step

  useAppEvent<number>('diff-change-step', (by) => step(by < 0 ? -1 : 1))

  // Only a new file or a new UI size builds the editor again. Every other prop updates it in place,
  // so the scroll position and the cursor stay.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new UI size builds the editor again, with the new font size.
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }
    // A new editor has its cursor at the top, whatever the old one had.
    lineRef.current = null
    changesRef.current = []
    lastReported.current = null

    const language = languageForPath(path)
    const editor = monaco.editor.createDiffEditor(element, {
      readOnly: true,
      originalEditable: false,
      automaticLayout: true,
      renderSideBySide: sideBySideRef.current,
      // The panel switches to inline itself below 720px and says why. Monaco must not switch on
      // its own at another width.
      useInlineViewWhenSpaceIsLimited: false,
      enableSplitViewResizing: true,
      renderOverviewRuler: true,
      renderIndicators: true,
      renderMarginRevertIcon: false,
      renderGutterMenu: false,
      diffCodeLens: false,
      diffAlgorithm: 'advanced',
      ignoreTrimWhitespace: ignoreRef.current,
      experimental: { showMoves: movesRef.current, showEmptyDecorations: true },
      hideUnchangedRegions: { ...UNCHANGED_REGIONS, enabled: collapseRef.current },
      diffWordWrap: 'inherit',
      wordWrap: wrapRef.current ? 'on' : 'off',
      stickyScroll: { enabled: true },
      minimap: { enabled: false },
      glyphMargin: false,
      lineNumbersMinChars: 3,
      scrollBeyondLastLine: false,
      lightbulb: { enabled: monaco.editor.ShowLightbulbIconMode.Off },
      scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 },
      fontFamily: TERMINAL_FONT,
      fontSize: 12 * uiZoom(),
      lineHeight: 18 * uiZoom(),
      theme: monacoTheme(themeRef.current),
    })
    editorRef.current = editor
    const texts = latestTexts.current
    shownTexts.current = texts
    const models: Models = {
      original: monaco.editor.createModel(texts.original, language),
      modified: monaco.editor.createModel(texts.modified, language),
    }
    modelsRef.current = models
    quiet.current = true
    try {
      editor.setModel(models)
    } finally {
      quiet.current = false
    }

    const modifiedEditor = editor.getModifiedEditor()
    const cursor = modifiedEditor.onDidChangeCursorPosition((event) => {
      if (quiet.current) {
        return
      }
      lineRef.current = event.position.lineNumber
      onLineRef.current?.(event.position.lineNumber)
      report()
    })
    let revealFirst = revealedPath.current !== path
    const updated = editor.onDidUpdateDiff(() => {
      const changes = editor.getLineChanges() ?? []
      changesRef.current = changes
      onStatsRef.current?.(countLineChanges(changes))
      const first = changes[0]
      if (revealFirst && first !== undefined) {
        // Show where the work is, without moving the cursor: a comment stays about the file.
        modifiedEditor.revealLineNearTop(changeAnchor(first).start)
        revealedPath.current = path
      }
      revealFirst = false
      report()
    })

    // Alt+F5 is also an app shortcut. This makes it work when the app does not see the key while
    // the editor has the focus; `stepping` keeps one press from stepping twice.
    const keys = [
      { id: 'next', by: 1, key: monaco.KeyMod.Alt | monaco.KeyCode.F5 },
      { id: 'previous', by: -1, key: monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.F5 },
    ] as const
    const actions = [editor.getOriginalEditor(), modifiedEditor].flatMap((inner) =>
      keys.map(({ id, by, key }) =>
        inner.addAction({
          id: `natamux.diff.${id}Change`,
          label: by === 1 ? 'Next change in the file' : 'Previous change in the file',
          keybindings: [key],
          run: () => stepRef.current(by),
        }),
      ),
    )

    const askAgent = (selection: CodeSelection) => onAskRef.current?.(selection)
    const asks = [
      attachAskAgent(editor.getOriginalEditor(), path, 'old', askAgent),
      attachAskAgent(modifiedEditor, path, 'new', askAgent),
    ]

    return () => {
      editorRef.current = null
      modelsRef.current = null
      shownTexts.current = null
      for (const disposable of [...actions, ...asks]) {
        disposable.dispose()
      }
      updated.dispose()
      cursor.dispose()
      // The editor must let go of the models before they are disposed.
      editor.setModel(null)
      models.original.dispose()
      models.modified.dispose()
      editor.dispose()
    }
  }, [path, uiSize])

  // The agent may change the file while it is open. New text goes into the same models, and the
  // view state is put back, so the reader keeps the place.
  useEffect(() => {
    const editor = editorRef.current
    const models = modelsRef.current
    const texts = shownTexts.current
    if (editor === null || models === null || texts === null) {
      return
    }
    if (texts.original === original && texts.modified === modified) {
      return
    }
    shownTexts.current = { original, modified }
    const view = editor.saveViewState()
    quiet.current = true
    try {
      if (texts.original !== original) {
        models.original.setValue(original)
      }
      if (texts.modified !== modified) {
        models.modified.setValue(modified)
      }
      if (view !== null) {
        editor.restoreViewState(view)
      }
    } finally {
      quiet.current = false
    }
  }, [original, modified])

  // The zones are added again when the comments change, and with a new editor.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new file or UI size builds a new editor.
  useEffect(() => {
    const editor = editorRef.current
    if (editor === null || reviewComments === undefined || reviewComments.length === 0) {
      return
    }
    const actions: ZoneActions = {
      onEdit: (id) => actionsRef.current?.onEdit(id),
      onDelete: (id) => actionsRef.current?.onDelete(id),
      onResolve: (id) => actionsRef.current?.onResolve(id),
      onReopen: (id) => actionsRef.current?.onReopen(id),
    }
    return showReviewZones(editor, reviewComments, sideBySide, actions)
  }, [reviewComments, sideBySide, path, uiSize])

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new file builds a new editor.
  useEffect(() => {
    const diff = editorRef.current
    if (diff !== null && reveal) {
      const inner = reveal.side === 'old' ? diff.getOriginalEditor() : diff.getModifiedEditor()
      inner.setPosition({ lineNumber: reveal.line, column: 1 })
      inner.revealLineInCenter(reveal.line)
    }
  }, [reveal, path])

  useEffect(() => {
    monaco.editor.setTheme(monacoTheme(theme))
  }, [theme])

  useEffect(() => {
    editorRef.current?.updateOptions({ wordWrap: wrap ? 'on' : 'off' })
  }, [wrap])

  useEffect(() => {
    editorRef.current?.updateOptions({ renderSideBySide: sideBySide })
  }, [sideBySide])

  useEffect(() => {
    editorRef.current?.updateOptions({
      hideUnchangedRegions: { ...UNCHANGED_REGIONS, enabled: collapseUnchanged },
    })
  }, [collapseUnchanged])

  useEffect(() => {
    editorRef.current?.updateOptions({ ignoreTrimWhitespace: ignoreWhitespace })
  }, [ignoreWhitespace])

  useEffect(() => {
    editorRef.current?.updateOptions({ experimental: { showMoves, showEmptyDecorations: true } })
  }, [showMoves])

  return (
    <section
      ref={container}
      aria-label={`Changes in ${path}`}
      className="min-h-64 min-w-0 flex-1 overflow-hidden"
    />
  )
}
