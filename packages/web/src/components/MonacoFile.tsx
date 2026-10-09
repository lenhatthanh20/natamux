import type { editor as MonacoEditor } from 'monaco-editor'
import { useEffect, useRef } from 'react'
import { trimTrailingNewline } from '../lib/changes'
import { MONO_FONT } from '../lib/fonts'
import { languageForPath } from '../lib/language'
import { DARK_THEME, LIGHT_THEME, monaco } from '../lib/monaco-setup'
import { uiZoom, useResolvedTheme } from '../lib/theme'
import { useUiSize } from '../lib/ui-size'

type MonacoFileProps = {
  path: string
  text: string
  // A new file is all added, a deleted file is all removed.
  kind: 'added' | 'removed'
  wrap: boolean
  onLine?: (line: number) => void
}

// Every line marked as added or removed.
function wholeFile(
  model: MonacoEditor.ITextModel,
  kind: 'added' | 'removed',
): MonacoEditor.IModelDeltaDecoration[] {
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

// A file that did not exist before (or is gone now) is not a diff. It is shown whole, read only,
// with every line marked, and without the empty last line that its final line break would add.
export default function MonacoFile({ path, text, kind, wrap, onLine }: MonacoFileProps) {
  const container = useRef<HTMLElement>(null)
  const editorRef = useRef<MonacoEditor.IStandaloneCodeEditor | null>(null)
  const decorationsRef = useRef<MonacoEditor.IEditorDecorationsCollection | null>(null)
  const latestText = useRef(text)
  latestText.current = text
  // The text the model holds, so a refresh with the same text costs nothing.
  const shownText = useRef<string | null>(null)
  // Monaco moves the cursor when the text is set. That move is not the user's.
  const quiet = useRef(false)
  const onLineRef = useRef(onLine)
  onLineRef.current = onLine
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
    shownText.current = latestText.current
    const model = monaco.editor.createModel(
      trimTrailingNewline(latestText.current),
      languageForPath(path),
    )
    const editor = monaco.editor.create(element, {
      model,
      readOnly: true,
      domReadOnly: true,
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
      fontFamily: MONO_FONT,
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

    return () => {
      editorRef.current = null
      decorationsRef.current = null
      shownText.current = null
      cursor.dispose()
      editor.setModel(null)
      model.dispose()
      editor.dispose()
    }
  }, [path, kind, uiSize])

  // The agent may change the file while it is open. The reader keeps the place.
  useEffect(() => {
    const editor = editorRef.current
    const model = editor?.getModel()
    if (!editor || !model || shownText.current === null || shownText.current === text) {
      return
    }
    shownText.current = text
    const view = editor.saveViewState()
    quiet.current = true
    try {
      model.setValue(trimTrailingNewline(text))
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
      aria-label={`${kind === 'added' ? 'New file' : 'Deleted file'} ${path}`}
      className="min-h-64 min-w-0 flex-1 overflow-hidden"
    />
  )
}
