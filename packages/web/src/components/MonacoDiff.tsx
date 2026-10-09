import type { editor as MonacoEditor } from 'monaco-editor'
import { useEffect, useRef } from 'react'
import { countLineChanges } from '../lib/changes'
import { MONO_FONT } from '../lib/fonts'
import { languageForPath } from '../lib/language'
import { DARK_THEME, LIGHT_THEME, monaco } from '../lib/monaco-setup'
import { type ResolvedTheme, uiZoom, useResolvedTheme } from '../lib/theme'
import { useUiSize } from '../lib/ui-size'

type MonacoDiffProps = {
  path: string
  original: string
  modified: string
  sideBySide: boolean
  wrap: boolean
  // Called each time the diff is worked out, with the lines added and removed.
  onStats?: (stats: { added: number; removed: number }) => void
  // The line of the new file that the cursor is on, for a comment about that line.
  onLine?: (line: number) => void
}

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
  onStats,
  onLine,
}: MonacoDiffProps) {
  const container = useRef<HTMLElement>(null)
  const editorRef = useRef<MonacoEditor.IStandaloneDiffEditor | null>(null)
  const onLineRef = useRef(onLine)
  onLineRef.current = onLine
  const onStatsRef = useRef(onStats)
  onStatsRef.current = onStats
  const wrapRef = useRef(wrap)
  wrapRef.current = wrap
  const theme = useResolvedTheme()
  const uiSize = useUiSize()
  const themeRef = useRef(theme)
  themeRef.current = theme

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new UI size builds the editor again, with the new font size.
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }

    const language = languageForPath(path)
    const editor = monaco.editor.createDiffEditor(element, {
      readOnly: true,
      originalEditable: false,
      renderSideBySide: sideBySide,
      automaticLayout: true,
      minimap: { enabled: false },
      renderOverviewRuler: false,
      scrollBeyondLastLine: false,
      glyphMargin: false,
      lightbulb: { enabled: monaco.editor.ShowLightbulbIconMode.Off },
      hideUnchangedRegions: { enabled: true },
      wordWrap: wrapRef.current ? 'on' : 'off',
      scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 },
      fontFamily: MONO_FONT,
      fontSize: 12 * uiZoom(),
      lineHeight: 18 * uiZoom(),
      theme: monacoTheme(themeRef.current),
    })
    editorRef.current = editor
    const originalModel = monaco.editor.createModel(original, language)
    const modifiedModel = monaco.editor.createModel(modified, language)
    editor.setModel({ original: originalModel, modified: modifiedModel })

    const cursor = editor
      .getModifiedEditor()
      .onDidChangeCursorPosition((event) => onLineRef.current?.(event.position.lineNumber))
    const updated = editor.onDidUpdateDiff(() =>
      onStatsRef.current?.(countLineChanges(editor.getLineChanges() ?? [])),
    )

    return () => {
      editorRef.current = null
      updated.dispose()
      cursor.dispose()
      // The editor must let go of the models before they are disposed.
      editor.setModel(null)
      originalModel.dispose()
      modifiedModel.dispose()
      editor.dispose()
    }
  }, [path, original, modified, sideBySide, uiSize])

  useEffect(() => {
    monaco.editor.setTheme(monacoTheme(theme))
  }, [theme])

  useEffect(() => {
    editorRef.current?.updateOptions({ wordWrap: wrap ? 'on' : 'off' })
  }, [wrap])

  return (
    <section
      ref={container}
      aria-label={`Changes in ${path}`}
      className="min-h-64 min-w-0 flex-1 overflow-hidden"
    />
  )
}
