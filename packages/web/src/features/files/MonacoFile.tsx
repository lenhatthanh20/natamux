import type { editor as MonacoEditor } from 'monaco-editor'
import { useEffect, useRef } from 'react'
import { type CodeSelection, trimTrailingNewline } from '../../lib/changes'
import { TERMINAL_FONT } from '../../lib/fonts'
import { uiZoom, useResolvedTheme } from '../../lib/theme'
import { useUiSize } from '../../lib/ui-size'
import { languageForPath } from './language'
import { attachAskAgent } from './monaco-ask'
import { DARK_THEME, LIGHT_THEME, monaco } from './monaco-setup'

type MonacoFileProps = {
  path: string
  text: string
  // A new file is all added, a deleted file is all removed. `edit`: a file of the worktree that
  // the user changes, with no marks.
  kind: 'added' | 'removed' | 'edit'
  wrap: boolean
  onLine?: (line: number) => void
  // Not for `edit`: asking the agent about lines is for the Changes tab.
  onAsk?: (selection: CodeSelection) => void
  // Edit only: the text after each change by the user, and Mod+S.
  onChange?: (text: string) => void
  onSave?: () => void
}

// Every line marked as added or removed.
function wholeFile(
  model: MonacoEditor.ITextModel,
  kind: MonacoFileProps['kind'],
): MonacoEditor.IModelDeltaDecoration[] {
  if (kind === 'edit') {
    return []
  }
  return [
    {
      range: new monaco.Range(1, 1, Math.max(model.getLineCount(), 1), 1),
      options: {
        isWholeLine: true,
        className: kind === 'added' ? 'hm-whole-added' : 'hm-whole-removed',
        linesDecorationsClassName: kind === 'added' ? 'hm-gutter-added' : 'hm-gutter-removed',
      },
    },
  ]
}

const LABEL = { added: 'New file', removed: 'Deleted file', edit: 'Edit' } as const

// The editor keeps the last line break, so a save writes back what was read.
function shownText(text: string, kind: MonacoFileProps['kind']): string {
  return kind === 'edit' ? text : trimTrailingNewline(text)
}

// A file that did not exist before (or is gone now) is not a diff. It is shown whole, read only,
// with every line marked, and without the empty last line that its final line break would add.
// The same view edits a file of the worktree (`edit`).
export default function MonacoFile({
  path,
  text,
  kind,
  wrap,
  onLine,
  onAsk,
  onChange,
  onSave,
}: MonacoFileProps) {
  const container = useRef<HTMLElement>(null)
  const editorRef = useRef<MonacoEditor.IStandaloneCodeEditor | null>(null)
  const decorationsRef = useRef<MonacoEditor.IEditorDecorationsCollection | null>(null)
  const latestText = useRef(text)
  latestText.current = text
  // The text the model holds, so a refresh with the same text costs nothing.
  const modelText = useRef<string | null>(null)
  // Monaco moves the cursor when the text is set. That move is not the user's.
  const quiet = useRef(false)
  const onLineRef = useRef(onLine)
  onLineRef.current = onLine
  const onAskRef = useRef(onAsk)
  onAskRef.current = onAsk
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange
  const onSaveRef = useRef(onSave)
  onSaveRef.current = onSave
  const theme = useResolvedTheme()
  const themeRef = useRef(theme)
  themeRef.current = theme
  const wrapRef = useRef(wrap)
  wrapRef.current = wrap
  const uiSize = useUiSize()

  // Only a new file or a new UI size builds the editor again. New text goes into the same model.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new UI size builds the editor again, with the new font size.
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }
    modelText.current = latestText.current
    const model = monaco.editor.createModel(
      shownText(latestText.current, kind),
      languageForPath(path),
    )
    const editing = kind === 'edit'
    const editor = monaco.editor.create(element, {
      model,
      readOnly: !editing,
      domReadOnly: !editing,
      automaticLayout: true,
      minimap: { enabled: false },
      overviewRulerLanes: 0,
      overviewRulerBorder: false,
      hideCursorInOverviewRuler: true,
      scrollBeyondLastLine: false,
      glyphMargin: false,
      folding: false,
      renderLineHighlight: 'none',
      lightbulb: { enabled: monaco.editor.ShowLightbulbIconMode.Off },
      lineDecorationsWidth: 16,
      wordWrap: wrapRef.current ? 'on' : 'off',
      scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 },
      fontFamily: TERMINAL_FONT,
      fontSize: 12 * uiZoom(),
      lineHeight: 18 * uiZoom(),
      theme: themeRef.current === 'dark' ? DARK_THEME : LIGHT_THEME,
    })
    editorRef.current = editor
    decorationsRef.current = editor.createDecorationsCollection(wholeFile(model, kind))
    const cursor = editor.onDidChangeCursorPosition((event) => {
      if (!quiet.current) {
        onLineRef.current?.(event.position.lineNumber)
      }
    })
    // The parent hands the typed text back; it is in the model already, so it is not set again.
    const typed = editor.onDidChangeModelContent(() => {
      if (!quiet.current) {
        modelText.current = model.getValue()
        onChangeRef.current?.(modelText.current)
      }
    })
    if (editing) {
      // Without it the browser would offer to save the page.
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => onSaveRef.current?.())
    }

    // A new file has only new lines, a deleted one only lines of the last commit.
    const askAgent = (selection: CodeSelection) => onAskRef.current?.(selection)
    const ask = editing
      ? null
      : attachAskAgent(editor, path, kind === 'added' ? 'new' : 'old', askAgent)

    return () => {
      editorRef.current = null
      decorationsRef.current = null
      modelText.current = null
      ask?.dispose()
      cursor.dispose()
      typed.dispose()
      editor.setModel(null)
      model.dispose()
      editor.dispose()
    }
  }, [path, kind, uiSize])

  // The agent may change the file while it is open. The reader keeps the place.
  useEffect(() => {
    const editor = editorRef.current
    const model = editor?.getModel()
    if (!editor || !model || modelText.current === null || modelText.current === text) {
      return
    }
    modelText.current = text
    const view = editor.saveViewState()
    quiet.current = true
    try {
      model.setValue(shownText(text, kind))
      decorationsRef.current?.set(wholeFile(model, kind))
      if (view !== null) {
        editor.restoreViewState(view)
      }
    } finally {
      quiet.current = false
    }
  }, [text, kind])

  useEffect(() => {
    monaco.editor.setTheme(theme === 'dark' ? DARK_THEME : LIGHT_THEME)
  }, [theme])

  useEffect(() => {
    editorRef.current?.updateOptions({ wordWrap: wrap ? 'on' : 'off' })
  }, [wrap])

  return (
    <section
      ref={container}
      aria-label={`${LABEL[kind]} ${path}`}
      className="min-h-64 min-w-0 flex-1 overflow-hidden"
    />
  )
}
