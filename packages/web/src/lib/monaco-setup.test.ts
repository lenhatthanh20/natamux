import { describe, expect, it } from 'vitest'
import languages from './language.ts?raw'
import source from './monaco-setup.ts?raw'

// Since 0.56 the exports map of monaco-editor maps "monaco-editor/x" to esm/vs/x.js. The old deep
// paths (esm/..., min/...) no longer resolve, and the build fails.
const IMPORT = /(?:from|import) '(monaco-editor[^']*)'/g
const DEEP_PATH = /^monaco-editor\/(esm|min|dev)\//

function monacoImports(text: string): string[] {
  return [...text.matchAll(IMPORT)].map((match) => match[1] ?? '')
}

// The language ids that lib/language.ts gives to files.
function languageIds(): string[] {
  return [...new Set([...languages.matchAll(/[:,] '([a-z]+)'/g)].map((match) => match[1] ?? ''))]
}

// The module that colors a language: most have their own, "c" comes with "cpp", and JSON is a
// language feature, not a basic language. Monaco has no Makefile language, so it stays plain text.
function contributionOf(id: string): string | null {
  if (id === 'plaintext' || id === 'makefile') {
    return null
  }
  if (id === 'json') {
    return 'monaco-editor/languages/features/json/register'
  }
  return `monaco-editor/languages/definitions/${id === 'c' ? 'cpp' : id}/register`
}

describe('the Monaco imports', () => {
  it('imports the editor API, not the full entry with every language and its services', () => {
    const imports = monacoImports(source)
    expect(imports).toContain('monaco-editor/editor/editor.api')
    expect(imports).not.toContain('monaco-editor')
  })

  it('uses only paths from the exports map of monaco-editor', () => {
    expect(monacoImports(source).filter((path) => DEEP_PATH.test(path))).toEqual([])
  })

  it('loads the icon font, which only the full entry loads by itself', () => {
    expect(source).toContain('/codicons/codicon/codicon.css')
  })

  it('reads the language ids of lib/language.ts', () => {
    expect(languageIds()).toEqual(expect.arrayContaining(['typescript', 'json', 'c', 'dockerfile']))
  })

  it('imports the colors of every language in lib/language.ts', () => {
    const imports = monacoImports(source)
    const wanted = languageIds().map(contributionOf)
    const missing = wanted.filter((path) => path !== null && !imports.includes(path))
    expect(missing).toEqual([])
  })

  it('imports the editor parts the diff and file views use', () => {
    expect(monacoImports(source)).toEqual(
      expect.arrayContaining([
        'monaco-editor/editor/browser/coreCommands',
        'monaco-editor/editor/browser/widget/diffEditor/diffEditor.contribution',
        'monaco-editor/editor/contrib/stickyScroll/browser/stickyScrollContribution',
        'monaco-editor/features/find/register',
      ]),
    )
  })

  // Diagnostics are off, so the language workers would only cost a download.
  it('loads only the editor worker', () => {
    const workers = monacoImports(source).filter((path) => path.endsWith('?worker'))
    expect(workers).toEqual(['monaco-editor/editor/editor.worker?worker'])
  })

  // JSON colors come from its language feature. Anything more would ask a JSON worker.
  it('keeps the JSON feature to colors only', () => {
    expect(source).toContain('jsonDefaults.setModeConfiguration({ tokens: true })')
  })
})
