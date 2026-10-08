// Loads Monaco, the editor of VS Code, from this app and not from a CDN. This file is only imported
// by the diff viewer, which is itself loaded the first time a diff is opened.
import * as monaco from 'monaco-editor'
import { typescript as monacoTypeScript } from 'monaco-editor'
import 'monaco-editor/min/vs/editor/editor.main.css'
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import cssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker'
import htmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker'
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker'
import tsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker'

globalThis.MonacoEnvironment = {
  getWorker(_workerId, label) {
    switch (label) {
      case 'json':
        return new jsonWorker()
      case 'css':
      case 'scss':
      case 'less':
        return new cssWorker()
      case 'html':
      case 'handlebars':
      case 'razor':
        return new htmlWorker()
      case 'typescript':
      case 'javascript':
        return new tsWorker()
      default:
        return new editorWorker()
    }
  },
}

// Monaco only shows code here. Its TypeScript checks do not know the rest of the project, so they
// would report errors that are not real (modules that cannot be found, and more). Colors stay.
const noDiagnostics = {
  noSemanticValidation: true,
  noSuggestionDiagnostics: true,
  noSyntaxValidation: true,
}
monacoTypeScript.typescriptDefaults.setDiagnosticsOptions(noDiagnostics)
monacoTypeScript.javascriptDefaults.setDiagnosticsOptions(noDiagnostics)

export const LIGHT_THEME = 'helmsman-light'
export const DARK_THEME = 'helmsman-dark'

// The background of the editor is the background of the panels around it.
monaco.editor.defineTheme(LIGHT_THEME, {
  base: 'vs',
  inherit: true,
  rules: [],
  colors: { 'editor.background': '#ffffff' },
})
monaco.editor.defineTheme(DARK_THEME, {
  base: 'vs-dark',
  inherit: true,
  rules: [],
  colors: { 'editor.background': '#101e2d' },
})

export { monaco }
