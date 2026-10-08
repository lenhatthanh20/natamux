import { useEffect, useRef } from 'react'
import { languageForPath } from '../lib/language'
import { DARK_THEME, LIGHT_THEME, monaco } from '../lib/monaco-setup'
import { type ResolvedTheme, uiZoom, useResolvedTheme } from '../lib/theme'
import { useUiSize } from '../lib/ui-size'

type MonacoDiffProps = {
  path: string
  original: string
  modified: string
  sideBySide: boolean
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
  onLine,
}: MonacoDiffProps) {
  const container = useRef<HTMLElement>(null)
  const onLineRef = useRef(onLine)
  onLineRef.current = onLine
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
      fontSize: 12 * uiZoom(),
      lineHeight: 18 * uiZoom(),
      theme: monacoTheme(themeRef.current),
    })
    const originalModel = monaco.editor.createModel(original, language)
    const modifiedModel = monaco.editor.createModel(modified, language)
    editor.setModel({ original: originalModel, modified: modifiedModel })

    const cursor = editor
      .getModifiedEditor()
      .onDidChangeCursorPosition((event) => onLineRef.current?.(event.position.lineNumber))

    return () => {
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

  return (
    <section
      ref={container}
      aria-label={`Changes in ${path}`}
      className="h-[70vh] min-h-[24rem] overflow-hidden rounded-md border border-line"
    />
  )
}
