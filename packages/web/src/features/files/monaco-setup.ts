// Loads Monaco, the editor of VS Code, from this app and not from a CDN. This file is only imported
// by the diff viewer, which is itself loaded the first time a diff is opened.
// The full entry (monaco-editor) brings about 80 languages, the TypeScript, CSS, HTML and JSON
// services and every editor feature. The views here only show code, so this loads the editor API,
// the parts the diff and file views use, and the colors of the languages in lib/language.ts.
import 'monaco-editor/editor/browser/coreCommands'
import 'monaco-editor/editor/browser/widget/diffEditor/diffEditor.contribution'
import 'monaco-editor/editor/contrib/clipboard/browser/clipboard'
import 'monaco-editor/editor/contrib/contextmenu/browser/contextmenu'
import 'monaco-editor/editor/contrib/folding/browser/folding'
import 'monaco-editor/editor/contrib/stickyScroll/browser/stickyScrollContribution'
import 'monaco-editor/editor/contrib/wordOperations/browser/wordOperations'
import * as monaco from 'monaco-editor/editor/editor.api'
import editorWorker from 'monaco-editor/editor/editor.worker?worker'
import 'monaco-editor/features/find/register'
import 'monaco-editor/languages/definitions/cpp/register'
import 'monaco-editor/languages/definitions/csharp/register'
import 'monaco-editor/languages/definitions/css/register'
import 'monaco-editor/languages/definitions/dockerfile/register'
import 'monaco-editor/languages/definitions/go/register'
import 'monaco-editor/languages/definitions/html/register'
import 'monaco-editor/languages/definitions/ini/register'
import 'monaco-editor/languages/definitions/java/register'
import 'monaco-editor/languages/definitions/javascript/register'
import 'monaco-editor/languages/definitions/kotlin/register'
import 'monaco-editor/languages/definitions/less/register'
import 'monaco-editor/languages/definitions/markdown/register'
import 'monaco-editor/languages/definitions/php/register'
import 'monaco-editor/languages/definitions/python/register'
import 'monaco-editor/languages/definitions/ruby/register'
import 'monaco-editor/languages/definitions/rust/register'
import 'monaco-editor/languages/definitions/scss/register'
import 'monaco-editor/languages/definitions/shell/register'
import 'monaco-editor/languages/definitions/sql/register'
import 'monaco-editor/languages/definitions/swift/register'
import 'monaco-editor/languages/definitions/typescript/register'
import 'monaco-editor/languages/definitions/xml/register'
import 'monaco-editor/languages/definitions/yaml/register'
// The exports map has no path for CSS, so the icon font comes from the package folder.
import '../../../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon-modifiers.css'
import '../../../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.css'

// The diff runs in the editor worker. Without the language services no other worker is needed.
// They are left out on purpose: a service loads later with editor parts (code lens, suggest) whose
// services the editor set up without, and every editor after it fails.
globalThis.MonacoEnvironment = { getWorker: () => new editorWorker() }

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

// Monaco measures a font once. JetBrains Mono may still be loading when the first code view opens,
// and characters measured with the fallback font put the cursor and selections off.
document.fonts
  ?.load('12px "JetBrains Mono Variable"')
  .then(() => monaco.editor.remeasureFonts())
  .catch(() => undefined)

export { monaco }
