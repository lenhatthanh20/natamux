import { type FormEvent, type KeyboardEvent, useLayoutEffect, useRef, useState } from 'react'
import { Button } from './ui/Button'
import { textarea } from './ui/styles'

type AgentCommentProps = {
  path: string
  // The line the cursor is on in the diff, when there is one.
  line?: number | null
  send: (text: string) => Promise<unknown>
}

// The text the agent gets: the file (and line) first, so the agent knows where to look.
export function commentForAgent(path: string, comment: string, line?: number | null): string {
  const where = line ? `${path}, line ${line}` : path
  return `Review comment on ${where}:\n${comment.trim()}`
}

// Four lines, then the field scrolls.
const MAX_FIELD_HEIGHT = 96

// A review comment on the file shown, pasted into the agent's terminal as its next prompt. It sits
// at the bottom of the diff column: one line that grows as the user types.
export function AgentComment({ path, line, send }: AgentCommentProps) {
  const [comment, setComment] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const field = useRef<HTMLTextAreaElement>(null)

  // biome-ignore lint/correctness/useExhaustiveDependencies: the text decides the height.
  useLayoutEffect(() => {
    const element = field.current
    if (element) {
      element.style.height = 'auto'
      element.style.height = `${Math.min(element.scrollHeight, MAX_FIELD_HEIGHT)}px`
    }
  }, [comment])

  const submit = async (event: { preventDefault: () => void }) => {
    event.preventDefault()
    if (comment.trim() === '' || sending) {
      return
    }
    setSending(true)
    setStatus(null)
    try {
      await send(commentForAgent(path, comment, line))
      setComment('')
      setStatus('Sent to the agent.')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setSending(false)
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      void submit(event)
    }
  }

  return (
    <form
      onSubmit={(event: FormEvent) => void submit(event)}
      className="flex shrink-0 flex-col gap-1 border-t border-line bg-surface px-3 py-2"
    >
      <div className="flex items-baseline gap-2">
        <label htmlFor="agent-comment" className="text-xs font-medium text-ink-2">
          {line ? `Comment on line ${line} for the agent` : 'Comment on this file for the agent'}
        </label>
        {status !== null && (
          <span role="status" className="min-w-0 truncate text-xs text-muted">
            {status}
          </span>
        )}
      </div>
      <div className="flex items-end gap-2">
        <textarea
          ref={field}
          id="agent-comment"
          rows={1}
          value={comment}
          onChange={(event) => setComment(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder="What should change, and why"
          className={`${textarea} min-h-(--control-h) flex-1 resize-none py-1`}
        />
        <Button
          type="submit"
          busy={sending}
          disabledReason={comment.trim() === '' ? 'Write a comment first' : undefined}
          shortcut="Mod+Enter"
        >
          Send to agent
        </Button>
      </div>
    </form>
  )
}
