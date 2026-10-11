// Loads Monaco, the editor of VS Code, from this app and not from a CDN. This file is only imported
// by the diff viewer, which is itself loaded the first time a diff is opened.
import * as monaco from 'monaco-editor'
import { typescript as monacoTypeScript } from 'monaco-editor'
import editorWorker from 'monaco-editor/editor/editor.worker?worker'
import cssWorker from 'monaco-editor/language/css/css.worker?worker'
import htmlWorker from 'monaco-editor/language/html/html.worker?worker'
import jsonWorker from 'monaco-editor/language/json/json.worker?worker'
import tsWorker from 'monaco-editor/language/typescript/ts.worker?worker'

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

export const LIGHT_THEME = 'natamux-light'
export const DARK_THEME = 'natamux-dark'

// Colors from the design tokens: the editor sits on the canvas, and the diff uses green and red
// washes that keep the text readable. The words that changed get a stronger wash on top of the
// line's, so they stand out in both themes. Moved code gets a neutral border, not a color of its
// own, because green and red already mean added and removed.
monaco.editor.defineTheme(LIGHT_THEME, {
  base: 'vs',
  inherit: true,
  rules: [],
  colors: {
    'editor.background': '#ffffff',
    'editorGutter.background': '#ffffff',
    'editorLineNumber.foreground': '#8a9099',
    'editorLineNumber.activeForeground': '#3f444c',
    'editor.lineHighlightBackground': '#00000000',
    'diffEditor.insertedLineBackground': '#2ea0431f',
    'diffEditor.removedLineBackground': '#f851491f',
    'diffEditor.insertedTextBackground': '#2ea04347',
    'diffEditor.removedTextBackground': '#f8514947',
    'diffEditorGutter.insertedLineBackground': '#2ea04333',
    'diffEditorGutter.removedLineBackground': '#f8514933',
    'diffEditor.unchangedRegionBackground': '#f6f7f9',
    'diffEditor.diagonalFill': '#14181f14',
    'diffEditor.border': '#e3e5e9',
    'diffEditor.unchangedRegionForeground': '#5f6570',
    'diffEditor.unchangedRegionShadow': '#14181f1f',
    'diffEditor.unchangedCodeBackground': '#14181f08',
    'diffEditor.move.border': '#8a909999',
    'diffEditor.moveActive.border': '#8a5a00',
    'diffEditorOverview.insertedForeground': '#2ea04399',
    'diffEditorOverview.removedForeground': '#f8514999',
    'editorOverviewRuler.border': '#00000000',
    'editorWidget.background': '#ffffff',
    'editorWidget.border': '#e3e5e9',
    'editorStickyScroll.background': '#ffffff',
    'editorStickyScroll.shadow': '#14181f1f',
    'scrollbarSlider.background': '#14181f33',
    'scrollbarSlider.hoverBackground': '#14181f5c',
    focusBorder: '#2f5fe0',
  },
})
monaco.editor.defineTheme(DARK_THEME, {
  base: 'vs-dark',
  inherit: true,
  rules: [],
  colors: {
    'editor.background': '#111214',
    'editorGutter.background': '#111214',
    'editorLineNumber.foreground': '#6b6f78',
    'editorLineNumber.activeForeground': '#b4b7be',
    'editor.lineHighlightBackground': '#00000000',
    'diffEditor.insertedLineBackground': '#2ea04329',
    'diffEditor.removedLineBackground': '#f8514929',
    'diffEditor.insertedTextBackground': '#2ea04352',
    'diffEditor.removedTextBackground': '#f8514952',
    'diffEditorGutter.insertedLineBackground': '#2ea0434d',
    'diffEditorGutter.removedLineBackground': '#f851494d',
    'diffEditor.diagonalFill': '#ffffff0d',
    'diffEditor.unchangedRegionBackground': '#16171a',
    'diffEditor.border': '#26282d',
    'diffEditor.unchangedRegionForeground': '#8d919a',
    'diffEditor.unchangedRegionShadow': '#00000066',
    'diffEditor.unchangedCodeBackground': '#ffffff06',
    'diffEditor.move.border': '#8d919a99',
    'diffEditor.moveActive.border': '#e8c15a',
    'diffEditorOverview.insertedForeground': '#3fb95099',
    'diffEditorOverview.removedForeground': '#f8514999',
    'editorOverviewRuler.border': '#00000000',
    'editorStickyScroll.background': '#111214',
    'editorStickyScroll.shadow': '#00000066',
    'scrollbarSlider.background': '#ffffff24',
    'scrollbarSlider.hoverBackground': '#ffffff42',
    'editorWidget.background': '#1c1d21',
    'editorWidget.border': '#363940',
    focusBorder: '#7aa2ff',
  },
})

export { monaco }
