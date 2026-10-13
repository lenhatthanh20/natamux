// Editor parts that monaco-editor ships without type files. monaco-setup.ts imports them only for
// what they register, and TypeScript checks that such an import is found.
declare module 'monaco-editor/editor/browser/coreCommands'
declare module 'monaco-editor/editor/browser/widget/diffEditor/diffEditor.contribution'
declare module 'monaco-editor/editor/contrib/clipboard/browser/clipboard'
declare module 'monaco-editor/editor/contrib/contextmenu/browser/contextmenu'
declare module 'monaco-editor/editor/contrib/folding/browser/folding'
declare module 'monaco-editor/editor/contrib/stickyScroll/browser/stickyScrollContribution'
declare module 'monaco-editor/editor/contrib/wordOperations/browser/wordOperations'
