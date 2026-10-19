import type { IDisposable, editor as MonacoEditor } from 'monaco-editor'
import { button } from '../../components/ui/styles'
import { type CodeSelection, linesLabel, wholeLines } from '../../lib/changes'
import { monaco } from './monaco-setup'

// An "Ask agent" button next to the selection of an editor, the same as Mod+Shift+A and as an
// entry of the editor's context menu. Each editor of a diff gets one, with its side.
export function attachAskAgent(
  editor: MonacoEditor.IStandaloneCodeEditor,
  path: string,
  side: CodeSelection['side'],
  onAsk: (selection: CodeSelection) => void,
): IDisposable {
  const picked = () => {
    const selection = editor.getSelection()
    return selection === null || selection.isEmpty() ? null : wholeLines(selection)
  }

  // The text is read only on ask: a drag changes the selection on every mouse move.
  const ask = () => {
    const lines = picked()
    const model = editor.getModel()
    if (lines === null || model === null) {
      return
    }
    const { start, end } = lines
    const range = new monaco.Range(start, 1, end, model.getLineMaxColumn(end))
    onAsk({ path, side, start, end, text: model.getValueInRange(range) })
  }

  const node = document.createElement('button')
  node.type = 'button'
  node.textContent = 'Ask agent'
  node.className = `${button('secondary', 'xs')} shadow-pop`
  node.dataset.tip = 'Ask the agent about the selected lines'
  node.dataset.tipKbd = 'Mod+Shift+A'
  // The editor must not see the press: it would move the cursor and drop the selection.
  node.addEventListener('pointerdown', (event) => event.stopPropagation())
  node.addEventListener('mousedown', (event) => {
    event.preventDefault()
    event.stopPropagation()
  })
  node.addEventListener('click', ask)

  const widget: MonacoEditor.IContentWidget = {
    getId: () => 'natamux.askAgent',
    getDomNode: () => node,
    getPosition: () => {
      const position = editor.getPosition()
      if (picked() === null || position === null) {
        return null
      }
      const { ABOVE, BELOW } = monaco.editor.ContentWidgetPositionPreference
      return { position, preference: [ABOVE, BELOW] }
    },
  }
  editor.addContentWidget(widget)
  const selection = editor.onDidChangeCursorSelection(() => {
    const lines = picked()
    if (lines !== null) {
      node.setAttribute('aria-label', `Ask the agent about ${linesLabel(lines)}`)
    }
    editor.layoutContentWidget(widget)
  })

  // Monaco stops the key once the action takes it, so the app's Mod+Shift+A (focus the comment
  // box) runs only when nothing is selected.
  const action = editor.addAction({
    id: 'natamux.askAgent',
    label: 'Ask agent about the selection',
    keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyA],
    precondition: 'editorHasSelection',
    contextMenuGroupId: 'navigation',
    run: ask,
  })

  return {
    dispose: () => {
      action.dispose()
      selection.dispose()
      editor.removeContentWidget(widget)
    },
  }
}
