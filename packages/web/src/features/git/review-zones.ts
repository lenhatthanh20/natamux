import type { ReviewComment } from '@natamux/core'
import type { editor as MonacoEditor } from 'monaco-editor'
import { button } from '../../components/ui/styles'
import { monaco } from '../files/monaco-setup'
import { actionsFor, commentLabel, stateLabel } from './review-comments'

// Body rows shown in a zone. A longer body shows "… more in the list".
const MAX_BODY_ROWS = 4

export type RevealLine = { side: 'old' | 'new'; line: number; n: number }

export type ZoneActions = {
  onEdit: (id: string) => void
  onDelete: (id: string) => void
  onResolve: (id: string) => void
  onReopen: (id: string) => void
}

const NAMES = { edit: 'Edit', delete: 'Delete', resolve: 'Resolve', reopen: 'Reopen' } as const

function row(text: string, height: number, className: string): HTMLElement {
  const node = document.createElement('div')
  node.className = `truncate px-2 ${className}`
  node.style.height = `${height}px`
  node.style.lineHeight = `${height}px`
  node.textContent = text
  return node
}

// Comment lines of the body that fit in the zone, with the cut marker when some are left out.
function bodyRows(body: string): string[] {
  const lines = body.split('\n')
  return lines.length > MAX_BODY_ROWS
    ? [...lines.slice(0, MAX_BODY_ROWS), '… more in the list']
    : lines
}

function zoneNode(
  comment: ReviewComment,
  lineHeight: number,
  actions: ZoneActions,
): { node: HTMLElement; rows: number } {
  const node = document.createElement('div')
  node.className = 'border-y border-line bg-surface text-xs text-ink'
  node.setAttribute('role', 'group')
  node.setAttribute('aria-label', `Comment ${comment.id}`)

  const header = document.createElement('div')
  header.className = 'flex items-center gap-1 px-2 text-ink-2'
  header.style.height = `${lineHeight}px`
  const title = document.createElement('span')
  title.className = 'min-w-0 flex-1 truncate'
  title.textContent = `${comment.id} · ${commentLabel(comment)} · ${stateLabel(comment.state)}`
  header.append(title)
  const run = {
    edit: () => actions.onEdit(comment.id),
    delete: () => actions.onDelete(comment.id),
    resolve: () => actions.onResolve(comment.id),
    reopen: () => actions.onReopen(comment.id),
  }
  for (const action of actionsFor(comment.state)) {
    const control = document.createElement('button')
    control.type = 'button'
    control.className = button('ghost', 'xs')
    control.textContent = NAMES[action]
    control.setAttribute('aria-label', `${NAMES[action]} ${comment.id}`)
    // The editor must not see the press: it would move the cursor.
    control.addEventListener('mousedown', (event) => {
      event.preventDefault()
      event.stopPropagation()
    })
    control.addEventListener('click', run[action])
    header.append(control)
  }
  node.append(header)

  const rows = bodyRows(comment.body)
  for (const text of rows) {
    node.append(row(text, lineHeight, 'whitespace-pre'))
  }
  return { node, rows: rows.length + 1 }
}

// Adds a view zone under the last line of each comment, on its side. Comments of the old side go
// on the original editor; in the inline view there is only one editor, so they are left to the
// list (its line numbers are not the modified text's). A whole-file or outdated comment has no
// line to sit under. Returns the function that removes the zones again.
export function showReviewZones(
  diff: MonacoEditor.IStandaloneDiffEditor,
  comments: readonly ReviewComment[],
  sideBySide: boolean,
  actions: ZoneActions,
): () => void {
  const added: { editor: MonacoEditor.ICodeEditor; id: string }[] = []
  for (const comment of comments) {
    if (comment.start === 0 || comment.outdated === true) {
      continue
    }
    if (comment.side === 'old' && !sideBySide) {
      continue
    }
    const editor = comment.side === 'old' ? diff.getOriginalEditor() : diff.getModifiedEditor()
    const lineHeight = editor.getOption(monaco.editor.EditorOption.lineHeight)
    const { node, rows } = zoneNode(comment, lineHeight, actions)
    editor.changeViewZones((accessor) => {
      const id = accessor.addZone({
        afterLineNumber: comment.end,
        heightInLines: rows,
        domNode: node,
        suppressMouseDown: true,
      })
      added.push({ editor, id })
    })
  }
  return () => {
    for (const { editor, id } of added) {
      try {
        editor.changeViewZones((accessor) => accessor.removeZone(id))
      } catch {
        // The editor is gone already, and its zones with it.
      }
    }
  }
}
