import { describe, expect, it } from 'vitest'
import source from './monaco-setup.ts?raw'

// Since 0.56 the exports map of monaco-editor maps "monaco-editor/x" to esm/vs/x.js. The old deep
// paths (esm/..., min/...) no longer resolve, and the build fails.
const IMPORT = /(?:from|import) '(monaco-editor[^']*)'/g
const DEEP_PATH = /^monaco-editor\/(esm|min|dev)\//

function monacoImports(text: string): string[] {
  return [...text.matchAll(IMPORT)].map((match) => match[1] ?? '')
}

describe('the Monaco imports', () => {
  it('reads the setup file', () => {
    expect(monacoImports(source)).toContain('monaco-editor')
  })

  it('uses only paths from the exports map of monaco-editor', () => {
    expect(monacoImports(source).filter((path) => DEEP_PATH.test(path))).toEqual([])
  })

  it('imports no CSS file, because the ESM modules load their own', () => {
    expect(monacoImports(source).filter((path) => path.endsWith('.css'))).toEqual([])
  })

  it('loads a worker for each language with its own service', () => {
    const workers = monacoImports(source).filter((path) => path.endsWith('?worker'))
    expect(workers.sort()).toEqual([
      'monaco-editor/editor/editor.worker?worker',
      'monaco-editor/language/css/css.worker?worker',
      'monaco-editor/language/html/html.worker?worker',
      'monaco-editor/language/json/json.worker?worker',
      'monaco-editor/language/typescript/ts.worker?worker',
    ])
  })
})
