import { useEffect, useRef } from 'react'
import { languageForPath } from '../lib/language'
import { DARK_THEME, LIGHT_THEME, monaco } from '../lib/monaco-setup'

type MonacoDiffProps = {
  path: string
  original: string
  modified: string
  sideBySide: boolean
  // The line of the new file that the cursor is on, for a comment about that line.
  onLine?: (line: number) => void
}

function themeForScheme(): string {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? DARK_THEME : LIGHT_THEME
}

// The diff of VS Code, read only. This default export is what ChangesView loads on demand.
export default function MonacoDiff({
  path,
  original,
  modified,
  sideBySide,
  onLine,
}: MonacoDiffProps) {
  const container = useRef<HTMLDivElement>(null)
  const onLineRef = useRef(onLine)
  onLineRef.current = onLine

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
      fontSize: 13,
      theme: themeForScheme(),
    })
    const originalModel = monaco.editor.createModel(original, language)
    const modifiedModel = monaco.editor.createModel(modified, language)
    editor.setModel({ original: originalModel, modified: modifiedModel })

    const cursor = editor
      .getModifiedEditor()
      .onDidChangeCursorPosition((event) => onLineRef.current?.(event.position.lineNumber))

    const scheme = window.matchMedia('(prefers-color-scheme: dark)')
    const followScheme = () => monaco.editor.setTheme(themeForScheme())
    scheme.addEventListener('change', followScheme)

    return () => {
      scheme.removeEventListener('change', followScheme)
      cursor.dispose()
      // The editor must let go of the models before they are disposed.
      editor.setModel(null)
      originalModel.dispose()
      modifiedModel.dispose()
      editor.dispose()
    }
  }, [path, original, modified, sideBySide])

  return (
    <div
      ref={container}
      title={`Changes in ${path}`}
      className="h-[70vh] min-h-[24rem] overflow-hidden rounded-md border border-line"
    />
  )
}
