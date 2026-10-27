import type { ReviewComment } from '@natamux/core'
import { useState } from 'react'
import { Button } from '../../components/ui/Button'
import { textarea } from '../../components/ui/styles'
import {
  actionsFor,
  commentLabel,
  draftCount,
  groupByFile,
  openCount,
  sendLabel,
  stateLabel,
} from './review-comments'
import type { useReviewComments } from './useReviewComments'

export type ReviewCommentsProps = {
  comments: readonly ReviewComment[]
  showResolved: boolean
  onShowResolved: (show: boolean) => void
  // The draft that is being edited here, or null.
  editingId: string | null
  onEditing: (id: string | null) => void
  onOpen: (comment: ReviewComment) => void
  onSave: (id: string, body: string) => Promise<boolean>
  onDelete: (id: string) => void
  onResolve: (id: string) => void
  onReopen: (id: string) => void
  onSend: () => void
  sending: boolean
  error: string | null
}

// Only the first lines of the quote: it tells which code the comment was about.
const QUOTE_LINES = 3

function EditBox({
  comment,
  onSave,
  onCancel,
}: {
  comment: ReviewComment
  onSave: (id: string, body: string) => Promise<boolean>
  onCancel: () => void
}) {
  const [body, setBody] = useState(comment.body)
  return (
    <div className="flex flex-col gap-1 px-3 pb-2">
      <textarea
        aria-label={`Edit ${comment.id}`}
        rows={2}
        value={body}
        onChange={(event) => setBody(event.target.value)}
        className={`${textarea} resize-none`}
      />
      <div className="flex gap-2">
        <Button
          size="xs"
          variant="primary"
          disabledReason={body.trim() === '' ? 'Write a comment first' : undefined}
          onClick={() => void onSave(comment.id, body)}
        >
          Save {comment.id}
        </Button>
        <Button size="xs" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  )
}

function Row({ comment, ...props }: { comment: ReviewComment } & ReviewCommentsProps) {
  const editing = props.editingId === comment.id
  const firstLine = comment.body.split('\n')[0] ?? ''
  const run = {
    edit: () => props.onEditing(comment.id),
    delete: () => props.onDelete(comment.id),
    resolve: () => props.onResolve(comment.id),
    reopen: () => props.onReopen(comment.id),
  }
  const names = { edit: 'Edit', delete: 'Delete', resolve: 'Resolve', reopen: 'Reopen' }
  return (
    <li className="border-b border-line last:border-b-0">
      <div className="flex items-center gap-1 px-3 pt-1.5">
        <button
          type="button"
          onClick={() => props.onOpen(comment)}
          className="min-w-0 flex-1 truncate text-left text-xs text-ink-2 hover:text-ink"
        >
          <span className="font-medium text-ink">{comment.id}</span> {commentLabel(comment)}
          <span className="text-muted"> · {stateLabel(comment.state)}</span>
          {comment.outdated === true && <span className="text-muted"> · outdated</span>}
        </button>
      </div>
      {!editing && <p className="truncate px-3 text-xs text-ink">{firstLine}</p>}
      {comment.outdated === true && comment.quote !== '' && (
        <pre className="mx-3 mt-0.5 max-h-12 overflow-hidden truncate rounded bg-raised px-1 text-xs text-muted">
          {comment.quote.split('\n').slice(0, QUOTE_LINES).join('\n')}
        </pre>
      )}
      {editing ? (
        <EditBox comment={comment} onSave={props.onSave} onCancel={() => props.onEditing(null)} />
      ) : (
        <div className="flex gap-1 px-3 pb-1.5 pt-0.5">
          {actionsFor(comment.state).map((action) => (
            <Button key={action} size="xs" variant="ghost" onClick={run[action]}>
              {`${names[action]} ${comment.id}`}
            </Button>
          ))}
        </div>
      )}
    </li>
  )
}

// The comments of a task, grouped by file, and the button that sends the drafts to the agent.
export function ReviewComments(props: ReviewCommentsProps) {
  const { comments, showResolved, onShowResolved, onSend, sending, error } = props
  const drafts = draftCount(comments)
  const groups = groupByFile(comments, showResolved)
  return (
    <section
      aria-label="Comments"
      className="flex max-h-72 shrink-0 flex-col border-t border-line bg-surface"
    >
      <div className="flex items-center gap-2 px-3 py-1.5">
        <h3 className="text-xs font-medium text-ink-2">Comments</h3>
        <span className="text-xs text-muted">{openCount(comments)} open</span>
        <label className="ml-auto flex items-center gap-1 text-xs text-ink-2">
          <input
            type="checkbox"
            checked={showResolved}
            onChange={(event) => onShowResolved(event.target.checked)}
          />
          Show resolved
        </label>
      </div>
      {groups.length > 0 && (
        <div className="min-h-0 overflow-y-auto">
          {groups.map((group) => (
            <div key={group.path}>
              <h4 className="truncate bg-raised px-3 py-0.5 text-xs text-muted">{group.path}</h4>
              <ul aria-label={`Comments on ${group.path}`}>
                {group.comments.map((comment) => (
                  <Row key={comment.id} comment={comment} {...props} />
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
      <div className="flex flex-col gap-1 p-2">
        <Button
          variant="primary"
          size="md"
          busy={sending}
          disabledReason={drafts === 0 ? 'Add a comment first. Only drafts are sent.' : undefined}
          shortcut="Mod+Alt+Enter"
          onClick={onSend}
        >
          {sendLabel(drafts)}
        </Button>
        {error !== null && (
          <p role="alert" className="text-xs text-danger">
            {error}
          </p>
        )}
      </div>
    </section>
  )
}

// The list wired to the state of useReviewComments.
export function ReviewCommentsSection({
  review,
  onOpen,
}: {
  review: ReturnType<typeof useReviewComments>
  onOpen: (comment: ReviewComment) => void
}) {
  return (
    <ReviewComments
      comments={review.comments}
      showResolved={review.showResolved}
      onShowResolved={review.setShowResolved}
      editingId={review.editingId}
      onEditing={review.setEditingId}
      onOpen={onOpen}
      onSave={review.save}
      onDelete={(id) => void review.remove(id)}
      onResolve={(id) => void review.resolve(id)}
      onReopen={(id) => void review.reopen(id)}
      onSend={() => void review.send()}
      sending={review.sending}
      error={review.error}
    />
  )
}
